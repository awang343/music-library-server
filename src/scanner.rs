use anyhow::{Context, Result};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::ItemKey;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "m4b", "mp4", "aac",
    "wav", "aiff", "aif", "wv", "ape", "mka",
];

#[derive(Debug, Default)]
pub struct ScanStats {
    pub seen: u64,
    pub inserted: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub removed: u64,
    pub failed: u64,
    pub failures: Vec<ScanFailure>,
}

#[derive(Debug)]
pub struct ScanFailure {
    pub path: PathBuf,
    pub reason: String,
}

pub async fn scan(pool: &SqlitePool, root: &Path) -> Result<ScanStats> {
    let mut stats = ScanStats::default();
    let mut seen_paths: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(root).follow_links(true) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "walk error");
                stats.failed += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let is_audio = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .map(|e| AUDIO_EXTS.contains(&e.as_str()))
            .unwrap_or(false);
        if !is_audio {
            continue;
        }

        stats.seen += 1;
        seen_paths.insert(entry.path().to_string_lossy().to_string());
        match scan_one(pool, entry.path()).await {
            Ok(ScanResult::Inserted) => stats.inserted += 1,
            Ok(ScanResult::Updated) => stats.updated += 1,
            Ok(ScanResult::Unchanged) => stats.unchanged += 1,
            Err(e) => {
                warn!(path = %entry.path().display(), error = ?e, "scan failed");
                stats.failed += 1;
                stats.failures.push(ScanFailure {
                    path: entry.path().to_path_buf(),
                    reason: format!("{e:#}"),
                });
            }
        }
    }

    stats.removed = remove_missing(pool, &seen_paths).await?;

    info!(
        seen = stats.seen,
        inserted = stats.inserted,
        updated = stats.updated,
        unchanged = stats.unchanged,
        removed = stats.removed,
        failed = stats.failed,
        "scan complete"
    );
    Ok(stats)
}

/// Remove DB rows whose path was not encountered during this scan.
/// Cascades clean up `track_tags` and `playlist_tracks` automatically.
async fn remove_missing(pool: &SqlitePool, seen: &HashSet<String>) -> Result<u64> {
    let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, path FROM tracks")
        .fetch_all(pool)
        .await?;

    let to_remove: Vec<(i64, String)> = rows
        .into_iter()
        .filter(|(_, p)| !seen.contains(p))
        .collect();

    if to_remove.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    for (id, path) in &to_remove {
        debug!(track_id = id, path = %path, "removing missing track");
        sqlx::query("DELETE FROM tracks WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    Ok(to_remove.len() as u64)
}

enum ScanResult {
    Inserted,
    Updated,
    Unchanged,
}

async fn scan_one(pool: &SqlitePool, path: &Path) -> Result<ScanResult> {
    let meta = std::fs::metadata(path).context("stat")?;
    let file_size = meta.len() as i64;
    let mtime = meta
        .modified()
        .context("mtime")?
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("mtime epoch")?
        .as_secs() as i64;
    let path_str = path.to_string_lossy().to_string();

    let existing: Option<(i64, i64, i64)> =
        sqlx::query_as("SELECT id, mtime, file_size FROM tracks WHERE path = ?")
            .bind(&path_str)
            .fetch_optional(pool)
            .await?;

    let existed = existing.is_some();
    if let Some((_, exist_mtime, exist_size)) = existing {
        if exist_mtime == mtime && exist_size == file_size {
            return Ok(ScanResult::Unchanged);
        }
    }

    let path_owned = path.to_path_buf();
    let parsed = tokio::task::spawn_blocking(move || parse_file(&path_owned))
        .await
        .context("blocking parse task")??;

    let now = chrono::Utc::now().timestamp();
    let mut tx = pool.begin().await?;

    let track_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO tracks (
            path, title, album, artist, album_artist,
            track_no, disc_no, duration_ms, year,
            bitrate, sample_rate, channels,
            file_size, mtime, added_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(path) DO UPDATE SET
            title        = excluded.title,
            album        = excluded.album,
            artist       = excluded.artist,
            album_artist = excluded.album_artist,
            track_no     = excluded.track_no,
            disc_no      = excluded.disc_no,
            duration_ms  = excluded.duration_ms,
            year         = excluded.year,
            bitrate      = excluded.bitrate,
            sample_rate  = excluded.sample_rate,
            channels     = excluded.channels,
            file_size    = excluded.file_size,
            mtime        = excluded.mtime,
            updated_at   = excluded.updated_at
        RETURNING id
        "#,
    )
    .bind(&path_str)
    .bind(&parsed.title)
    .bind(&parsed.album)
    .bind(&parsed.artist)
    .bind(&parsed.album_artist)
    .bind(parsed.track_no)
    .bind(parsed.disc_no)
    .bind(parsed.duration_ms)
    .bind(parsed.year)
    .bind(parsed.bitrate)
    .bind(parsed.sample_rate)
    .bind(parsed.channels)
    .bind(file_size)
    .bind(mtime)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    // Wipe non-user tags for this track, then reinsert imported ones.
    sqlx::query("DELETE FROM track_tags WHERE track_id = ? AND source != 'user'")
        .bind(track_id)
        .execute(&mut *tx)
        .await?;

    for (ns, val) in &parsed.tags {
        let tag_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO tags (namespace, value) VALUES (?, ?)
            ON CONFLICT(namespace, value) DO UPDATE SET namespace = namespace
            RETURNING id
            "#,
        )
        .bind(ns)
        .bind(val)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT OR IGNORE INTO track_tags (track_id, tag_id, source, added_at) VALUES (?, ?, 'file', ?)",
        )
        .bind(track_id)
        .bind(tag_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(if existed {
        ScanResult::Updated
    } else {
        ScanResult::Inserted
    })
}

#[derive(Debug, Default)]
struct Parsed {
    title: Option<String>,
    album: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    track_no: Option<i64>,
    disc_no: Option<i64>,
    duration_ms: Option<i64>,
    year: Option<i64>,
    bitrate: Option<i64>,
    sample_rate: Option<i64>,
    channels: Option<i64>,
    tags: Vec<(String, String)>,
}

fn parse_file(path: &Path) -> Result<Parsed> {
    let probe = Probe::open(path)
        .with_context(|| format!("probe::open {}", path.display()))?
        .read()
        .with_context(|| format!("probe::read {}", path.display()))?;

    let props = probe.properties();
    let mut out = Parsed {
        duration_ms: Some(props.duration().as_millis() as i64),
        bitrate: props.audio_bitrate().map(|b| b as i64),
        sample_rate: props.sample_rate().map(|s| s as i64),
        channels: props.channels().map(|c| c as i64),
        ..Default::default()
    };

    let tag = probe.primary_tag().or_else(|| probe.first_tag());
    if let Some(tag) = tag {
        out.title = tag.get_string(ItemKey::TrackTitle).map(str::to_owned);
        out.album = tag.get_string(ItemKey::AlbumTitle).map(str::to_owned);
        out.artist = tag.get_string(ItemKey::TrackArtist).map(str::to_owned);
        out.album_artist = tag.get_string(ItemKey::AlbumArtist).map(str::to_owned);
        out.track_no = tag
            .get_string(ItemKey::TrackNumber)
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.trim().parse().ok());
        out.disc_no = tag
            .get_string(ItemKey::DiscNumber)
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.trim().parse().ok());
        out.year = tag
            .get_string(ItemKey::Year)
            .and_then(|s| s.trim().parse().ok())
            .or_else(|| {
                tag.get_string(ItemKey::RecordingDate)
                    .and_then(|s| s.get(..4))
                    .and_then(|y| y.parse().ok())
            });

        for item in tag.items() {
            let ns: &'static str = match item.key() {
                ItemKey::Genre => "genre",
                ItemKey::Composer => "composer",
                _ => continue,
            };
            if let Some(v) = item.value().text() {
                let v = v.trim();
                if !v.is_empty() {
                    out.tags.push((ns.to_string(), v.to_string()));
                }
            }
        }
    }

    Ok(out)
}
