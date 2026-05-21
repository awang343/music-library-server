use crate::config::LibraryConfig;
use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub root_path: String,
}

impl Library {
    pub fn root(&self) -> PathBuf {
        PathBuf::from(&self.root_path)
    }
}

/// Reconcile the libraries table with config. Inserts missing libraries by
/// name, updates root_path on existing rows. Renames the legacy placeholder
/// row (id=1, name='__pending__') to the first config entry so pre-existing
/// tracks/playlists land in a real library.
pub async fn sync(pool: &SqlitePool, cfg: &[LibraryConfig]) -> Result<Vec<Library>> {
    let mut tx = pool.begin().await?;

    // Reassign the legacy placeholder to the first config library, if present.
    let placeholder_exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM libraries WHERE id = 1 AND name = '__pending__'")
            .fetch_optional(&mut *tx)
            .await
            .context("checking placeholder library")?;
    if placeholder_exists.is_some() {
        if let Some(first) = cfg.first() {
            sqlx::query("UPDATE libraries SET name = ?, root_path = ? WHERE id = 1")
                .bind(first.name.trim())
                .bind(first.path.to_string_lossy().to_string())
                .execute(&mut *tx)
                .await
                .context("renaming placeholder library")?;
        }
    }

    for lib in cfg {
        let name = lib.name.trim();
        let path = lib.path.to_string_lossy().to_string();
        sqlx::query(
            "INSERT INTO libraries (name, root_path) VALUES (?, ?) \
             ON CONFLICT(name) DO UPDATE SET root_path = excluded.root_path",
        )
        .bind(name)
        .bind(&path)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("upserting library {name}"))?;
    }

    tx.commit().await?;

    // Return all libraries that match the config, in config order.
    let mut out = Vec::with_capacity(cfg.len());
    for lib in cfg {
        let row: Library = sqlx::query_as(
            "SELECT id, name, root_path FROM libraries WHERE name = ?",
        )
        .bind(lib.name.trim())
        .fetch_one(pool)
        .await
        .with_context(|| format!("loading library {}", lib.name))?;
        out.push(row);
    }
    Ok(out)
}
