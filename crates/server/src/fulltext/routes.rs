//! HTTP handlers for the full-text index.
//!
//! * `GET /api/index/status` — public: filenames, states and counters only.
//! * `POST /api/index/:filename/build` — admin (protected router).
//! * `DELETE /api/index/:filename` — admin (protected router).

use super::worker::{BuildOutcome, IndexError, IndexStatusResponse};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// How long a delete waits for a running job on that archive to stop.
const REMOVE_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Serialize)]
pub struct BuildResponse {
    pub filename: String,
    pub outcome: BuildOutcome,
}

fn error_response(error: IndexError) -> (StatusCode, String) {
    match error {
        IndexError::Disabled => (StatusCode::CONFLICT, "Full-text indexing is disabled (FYR_FULLTEXT=off)".to_string()),
        IndexError::InvalidName => (StatusCode::BAD_REQUEST, "Not a ZIM archive filename".to_string()),
        IndexError::NotFound => (StatusCode::NOT_FOUND, "Archive not found".to_string()),
        IndexError::Busy => (StatusCode::CONFLICT, "The index is in use; try again shortly".to_string()),
    }
}

fn blocking_failed(status: StatusCode) -> (StatusCode, String) {
    (status, "Index request failed".to_string())
}

/// GET /api/index/status
pub async fn index_status(State(state): State<Arc<AppState>>) -> Result<Json<IndexStatusResponse>, StatusCode> {
    let indexer = Arc::clone(&state.fulltext);
    crate::handlers::run_blocking(move || Ok(Json(indexer.status()))).await
}

/// POST /api/index/:filename/build
pub async fn build_index(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<(StatusCode, Json<BuildResponse>), (StatusCode, String)> {
    let indexer = Arc::clone(&state.fulltext);
    let name = filename.clone();
    let outcome = crate::handlers::run_blocking(move || Ok(indexer.request_build(&name)))
        .await
        .map_err(blocking_failed)?
        .map_err(error_response)?;
    let status = match outcome {
        BuildOutcome::AlreadyReady => StatusCode::OK,
        _ => StatusCode::ACCEPTED,
    };
    Ok((status, Json(BuildResponse { filename, outcome })))
}

/// DELETE /api/index/:filename
pub async fn delete_index(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let indexer = Arc::clone(&state.fulltext);
    crate::handlers::run_blocking(move || Ok(indexer.remove(&filename, REMOVE_WAIT)))
        .await
        .map_err(blocking_failed)?
        .map_err(error_response)?;
    Ok(StatusCode::NO_CONTENT)
}
