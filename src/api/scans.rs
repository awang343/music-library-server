use crate::api::error::{ApiError, ApiResult};
use crate::api::SharedState;
use crate::scanner;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanState {
    pub running: bool,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub last_stats: Option<ScanStatsView>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanStatsView {
    pub seen: u64,
    pub inserted: u64,
    pub updated: u64,
    pub unchanged: u64,
    pub removed: u64,
    pub failed: u64,
}

impl From<&scanner::ScanStats> for ScanStatsView {
    fn from(s: &scanner::ScanStats) -> Self {
        Self {
            seen: s.seen,
            inserted: s.inserted,
            updated: s.updated,
            unchanged: s.unchanged,
            removed: s.removed,
            failed: s.failed,
        }
    }
}

pub fn routes() -> Router<SharedState> {
    Router::new().route("/scans", get(get_status).post(trigger))
}

async fn get_status(
    State(state): State<SharedState>,
    Path(lib_id): Path<i64>,
) -> ApiResult<Json<ScanState>> {
    state.require_library(lib_id)?;
    let snap = state
        .scan_states
        .lock()
        .expect("scan_states poisoned")
        .get(&lib_id)
        .cloned()
        .unwrap_or_default();
    Ok(Json(snap))
}

async fn trigger(
    State(state): State<SharedState>,
    Path(lib_id): Path<i64>,
) -> ApiResult<(StatusCode, Json<ScanState>)> {
    let lib = state.require_library(lib_id)?;
    {
        let mut map = state.scan_states.lock().expect("scan_states poisoned");
        let s = map.entry(lib_id).or_default();
        if s.running {
            return Err(ApiError {
                status: StatusCode::CONFLICT,
                message: "scan already running".into(),
            });
        }
        s.running = true;
        s.started_at = Some(chrono::Utc::now().timestamp());
        s.finished_at = None;
        s.last_error = None;
    }

    let bg = state.clone();
    let root = lib.root();
    tokio::spawn(async move {
        let result = scanner::scan(&bg.pool, lib_id, &root).await;
        let mut map = bg.scan_states.lock().expect("scan_states poisoned");
        let s = map.entry(lib_id).or_default();
        s.running = false;
        s.finished_at = Some(chrono::Utc::now().timestamp());
        match result {
            Ok(stats) => {
                s.last_stats = Some(ScanStatsView::from(&stats));
            }
            Err(e) => {
                s.last_error = Some(format!("{e:#}"));
            }
        }
    });

    let snap = state
        .scan_states
        .lock()
        .expect("scan_states poisoned")
        .get(&lib_id)
        .cloned()
        .unwrap_or_default();
    Ok((StatusCode::ACCEPTED, Json(snap)))
}
