use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub mod auth;
pub mod error;
pub mod playlists;
pub mod search;
pub mod stream;
pub mod tags;
pub mod tracks;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub auth_token: Option<String>,
}

pub type SharedState = Arc<AppState>;

pub fn router(pool: SqlitePool, auth_token: Option<String>) -> Router {
    let state = Arc::new(AppState { pool, auth_token });

    let protected = Router::new()
        .merge(tracks::routes())
        .merge(stream::routes())
        .merge(tags::routes())
        .merge(search::routes())
        .merge(playlists::routes())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_bearer,
        ));

    let open = Router::new().route("/health", get(|| async { Json(json!({ "ok": true })) }));

    Router::new()
        .merge(open)
        .merge(protected)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
