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
        .route("/api/tracks/:id/tags", get(list_track_tags).post(add_user_tag))
        .route("/api/tracks/:id/tags/:tag_id", delete(remove_user_tag))
        .route("/api/tags", get(list_tags))
}

#[derive(Debug, Serialize, FromRow)]
pub struct TrackTagRow {
    pub tag_id: i64,
    pub namespace: String,
    pub value: String,
    pub source: String,
    pub added_at: i64,
}

async fn list_track_tags(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<TrackTagRow>>> {
    let rows = sqlx::query_as::<_, TrackTagRow>(
        "SELECT t.id AS tag_id, t.namespace, t.value, tt.source, tt.added_at \
         FROM track_tags tt JOIN tags t ON t.id = tt.tag_id \
         WHERE tt.track_id = ? \
         ORDER BY t.namespace, t.value, tt.source",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct NewTag {
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct AddedTag {
    pub tag_id: i64,
    pub namespace: String,
    pub value: String,
}

async fn add_user_tag(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Json(body): Json<NewTag>,
) -> ApiResult<(StatusCode, Json<AddedTag>)> {
    let namespace = body.namespace.trim();
    let value = body.value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request("value must be non-empty"));
    }

    // Confirm the track exists.
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM tracks WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::not_found("track"));
    }

    let now = chrono::Utc::now().timestamp();
    let mut tx = state.pool.begin().await?;

    let tag_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO tags (namespace, value) VALUES (?, ?)
        ON CONFLICT(namespace, value) DO UPDATE SET namespace = namespace
        RETURNING id
        "#,
    )
    .bind(namespace)
    .bind(value)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO track_tags (track_id, tag_id, source, added_at) \
         VALUES (?, ?, 'user', ?)",
    )
    .bind(id)
    .bind(tag_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(AddedTag {
            tag_id,
            namespace: namespace.to_string(),
            value: value.to_string(),
        }),
    ))
}

async fn remove_user_tag(
    State(state): State<SharedState>,
    Path((track_id, tag_id)): Path<(i64, i64)>,
) -> ApiResult<StatusCode> {
    let res = sqlx::query(
        "DELETE FROM track_tags WHERE track_id = ? AND tag_id = ? AND source = 'user'",
    )
    .bind(track_id)
    .bind(tag_id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("user tag on track"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, FromRow)]
pub struct TagCount {
    pub tag_id: i64,
    pub namespace: String,
    pub value: String,
    pub track_count: i64,
}

async fn list_tags(State(state): State<SharedState>) -> ApiResult<Json<Vec<TagCount>>> {
    let rows = sqlx::query_as::<_, TagCount>(
        "SELECT t.id AS tag_id, t.namespace, t.value, COUNT(DISTINCT tt.track_id) AS track_count \
         FROM tags t LEFT JOIN track_tags tt ON tt.tag_id = t.id \
         GROUP BY t.id ORDER BY track_count DESC, t.namespace, t.value",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}
