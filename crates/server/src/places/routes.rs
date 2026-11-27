//! HTTP handlers for the place index.
//!
//! * `GET /api/places/status` — public: filenames, states and counters only.
//! * `POST /api/places/:filename/build` — admin (protected router).
//! * `DELETE /api/places/:filename` — admin (protected router).
//!
//! Searching places goes through global search (`/api/search?kinds=place`).

use super::worker::{BuildOutcome, PlacesError, PlacesStatusResponse};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

/// How long a delete waits for a running job on that map to stop.
const REMOVE_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Serialize)]
pub struct BuildResponse {
    pub filename: String,
    pub outcome: BuildOutcome,
}

fn error_response(error: PlacesError) -> (StatusCode, String) {
    match error {
        PlacesError::Disabled => (StatusCode::CONFLICT, "Place indexing is disabled (FYR_PLACES=off)".to_string()),
        PlacesError::InvalidName => (StatusCode::BAD_REQUEST, "Not a map filename".to_string()),
        PlacesError::NotFound => (StatusCode::NOT_FOUND, "Map not found".to_string()),
        PlacesError::Unsupported(reason) => (StatusCode::UNPROCESSABLE_ENTITY, reason),
        PlacesError::Busy => (StatusCode::CONFLICT, "The index is in use; try again shortly".to_string()),
    }
}

fn blocking_failed(status: StatusCode) -> (StatusCode, String) {
    (status, "Place index request failed".to_string())
}

/// GET /api/places/status
pub async fn places_status(State(state): State<Arc<AppState>>) -> Result<Json<PlacesStatusResponse>, StatusCode> {
    let indexer = Arc::clone(&state.places);
    crate::handlers::run_blocking(move || Ok(Json(indexer.status()))).await
}

/// POST /api/places/:filename/build
pub async fn build_places(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<(StatusCode, Json<BuildResponse>), (StatusCode, String)> {
    let indexer = Arc::clone(&state.places);
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

/// DELETE /api/places/:filename
pub async fn delete_places(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let indexer = Arc::clone(&state.places);
    crate::handlers::run_blocking(move || Ok(indexer.remove(&filename, REMOVE_WAIT)))
        .await
        .map_err(blocking_failed)?
        .map_err(error_response)?;
    Ok(StatusCode::NO_CONTENT)
}
