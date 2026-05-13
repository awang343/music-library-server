use crate::api::error::{ApiError, ApiResult};
use crate::api::SharedState;
use crate::scanner;
use axum::extract::State;
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
    pub failed: u64,
}

impl From<&scanner::ScanStats> for ScanStatsView {
    fn from(s: &scanner::ScanStats) -> Self {
        Self {
            seen: s.seen,
            inserted: s.inserted,
            updated: s.updated,
            unchanged: s.unchanged,
            failed: s.failed,
        }
    }
}

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/api/scans", get(get_status).post(trigger))
}

async fn get_status(State(state): State<SharedState>) -> Json<ScanState> {
    let snapshot = state.scan_state.lock().expect("scan_state poisoned").clone();
    Json(snapshot)
}

async fn trigger(State(state): State<SharedState>) -> ApiResult<(StatusCode, Json<ScanState>)> {
    {
        let mut s = state.scan_state.lock().expect("scan_state poisoned");
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
    tokio::spawn(async move {
        let result = scanner::scan(&bg.pool, &bg.library_path).await;
        let mut s = bg.scan_state.lock().expect("scan_state poisoned");
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

    let snapshot = state.scan_state.lock().expect("scan_state poisoned").clone();
    Ok((StatusCode::ACCEPTED, Json(snapshot)))
}
