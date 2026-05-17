use crate::api::error::{ApiError, ApiResult};
use crate::api::SharedState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/playlists", get(list).post(create))
        .route(
            "/api/playlists/{id}",
            get(get_one).patch(update).delete(delete_one),
        )
        .route("/api/playlists/{id}/tracks", get(get_tracks).post(add_track).put(set_tracks))
        .route(
            "/api/playlists/{id}/tracks/{track_id}",
            delete(remove_track),
        )
}

#[derive(Debug, Serialize, FromRow)]
pub struct PlaylistSummary {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub track_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PlaylistRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PlaylistTrack {
    pub track_id: i64,
    pub position: i64,
    pub added_at: i64,
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PlaylistWithTracks {
    #[serde(flatten)]
    pub playlist: PlaylistRow,
    pub tracks: Vec<PlaylistTrack>,
}

#[derive(Debug, Deserialize)]
pub struct NewPlaylist {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchPlaylist {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AddTrackBody {
    pub track_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct SetTracksBody {
    pub track_ids: Vec<i64>,
}

async fn list(State(state): State<SharedState>) -> ApiResult<Json<Vec<PlaylistSummary>>> {
    let rows = sqlx::query_as::<_, PlaylistSummary>(
        "SELECT p.id, p.name, p.description, \
         COALESCE((SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = p.id), 0) AS track_count, \
         p.created_at, p.updated_at \
         FROM playlists p ORDER BY p.name",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn create(
    State(state): State<SharedState>,
    Json(body): Json<NewPlaylist>,
) -> ApiResult<(StatusCode, Json<PlaylistRow>)> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name must be non-empty"));
    }
    let now = chrono::Utc::now().timestamp();
    let row = sqlx::query_as::<_, PlaylistRow>(
        "INSERT INTO playlists (name, description, created_at, updated_at) \
         VALUES (?, ?, ?, ?) \
         RETURNING id, name, description, created_at, updated_at",
    )
    .bind(name)
    .bind(body.description.as_deref())
    .bind(now)
    .bind(now)
    .fetch_one(&state.pool)
    .await
    .map_err(map_unique)?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn get_one(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PlaylistWithTracks>> {
    let playlist = sqlx::query_as::<_, PlaylistRow>(
        "SELECT id, name, description, created_at, updated_at FROM playlists WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("playlist"))?;

    let tracks = fetch_playlist_tracks(&state, id).await?;
    Ok(Json(PlaylistWithTracks { playlist, tracks }))
}

async fn get_tracks(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<PlaylistTrack>>> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM playlists WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::not_found("playlist"));
    }
    Ok(Json(fetch_playlist_tracks(&state, id).await?))
}

async fn fetch_playlist_tracks(
    state: &SharedState,
    playlist_id: i64,
) -> Result<Vec<PlaylistTrack>, ApiError> {
    let rows = sqlx::query_as::<_, PlaylistTrack>(
        "SELECT pt.track_id, pt.position, pt.added_at, \
                t.title, t.album, t.artist, t.album_artist, t.duration_ms \
         FROM playlist_tracks pt JOIN tracks t ON t.id = pt.track_id \
         WHERE pt.playlist_id = ? \
         ORDER BY pt.position",
    )
    .bind(playlist_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
}

async fn update(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Json(body): Json<PatchPlaylist>,
) -> ApiResult<Json<PlaylistRow>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name must be non-empty"));
    }
    let now = chrono::Utc::now().timestamp();
    let row = sqlx::query_as::<_, PlaylistRow>(
        "UPDATE playlists SET name = ?, updated_at = ? WHERE id = ? \
         RETURNING id, name, description, created_at, updated_at",
    )
    .bind(name)
    .bind(now)
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(map_unique)?
    .ok_or_else(|| ApiError::not_found("playlist"))?;
    Ok(Json(row))
}

async fn delete_one(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let res = sqlx::query("DELETE FROM playlists WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("playlist"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn add_track(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Json(body): Json<AddTrackBody>,
) -> ApiResult<StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let mut tx = state.pool.begin().await?;

    let pl_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM playlists WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if pl_exists.is_none() {
        return Err(ApiError::not_found("playlist"));
    }
    let tr_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM tracks WHERE id = ?")
        .bind(body.track_id)
        .fetch_optional(&mut *tx)
        .await?;
    if tr_exists.is_none() {
        return Err(ApiError::not_found("track"));
    }

    let max_pos: Option<i64> =
        sqlx::query_scalar("SELECT MAX(position) FROM playlist_tracks WHERE playlist_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    let new_pos = max_pos.map(|p| p + 1).unwrap_or(0);

    sqlx::query(
        "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position, added_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(body.track_id)
    .bind(new_pos)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(StatusCode::CREATED)
}

async fn remove_track(
    State(state): State<SharedState>,
    Path((id, track_id)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "DELETE FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?",
    )
    .bind(id)
    .bind(track_id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("track in playlist"));
    }
    sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_tracks(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Json(body): Json<SetTracksBody>,
) -> ApiResult<StatusCode> {
    let now = chrono::Utc::now().timestamp();
    let mut tx = state.pool.begin().await?;

    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM playlists WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(ApiError::not_found("playlist"));
    }

    // Detect duplicates in the request — PK would reject anyway, fail early with a nicer message.
    let mut seen = std::collections::HashSet::new();
    for tid in &body.track_ids {
        if !seen.insert(*tid) {
            return Err(ApiError::bad_request(format!(
                "duplicate track_id {tid} in track_ids"
            )));
        }
    }

    sqlx::query("DELETE FROM playlist_tracks WHERE playlist_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    for (pos, track_id) in body.track_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(*track_id)
        .bind(pos as i64)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db) = e {
                if db.message().contains("FOREIGN KEY") {
                    return ApiError::bad_request(format!("unknown track_id {track_id}"));
                }
            }
            e.into()
        })?;
    }

    sqlx::query("UPDATE playlists SET updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn map_unique(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db) = e {
        if db.is_unique_violation() {
            return ApiError::bad_request("playlist name already exists");
        }
    }
    e.into()
}
