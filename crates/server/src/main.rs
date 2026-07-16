//! Offline Nexus — Off-Grid Content Platform
//!
//! A self-contained Rust application for offline content distribution and consumption.
//! Supports maps (PMTiles), books (EPUB), and POIs (FlatGeoBuf/GeoJSON).

use axum::{
    routing::{get, post},
    Router,
};
use types::Config;
use downloader::DownloadManager;
use std::sync::Arc;
use tracing::info;

mod handlers;
mod state;

pub use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Offline Nexus");

    // Initialize configuration
    let config = Config::default();
    config.initialize_directories()?;

    info!("Data directory: {}", config.data_dir.display());
    info!("Server will run on: {}:{}", config.server.host, config.server.port);

    // Create shared application state
    let app_state = AppState {
        config: Arc::new(config),
        download_manager: Arc::new(DownloadManager::new()),
    };

    // Build router
    let app = create_router(app_state);

    // Run server
    let listener = tokio::net::TcpListener::bind(format!(
        "{}:{}",
        "127.0.0.1", 8080
    ))
    .await?;

    info!("Server listening on http://127.0.0.1:8080");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the API router with all endpoints
fn create_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .route("/api/status", get(handlers::status))
        .route("/api/config", get(handlers::config))
        .route("/api/content/maps", get(handlers::list_maps))
        .route("/api/content/books", get(handlers::list_books))
        .route("/api/content/poi", get(handlers::list_poi))
        .route("/api/download", post(handlers::create_download))
        .route("/api/download/:task_id/status", get(handlers::get_download_status))
        .route("/api/downloads", get(handlers::list_downloads))
        // Static file serving (UI)
        .fallback(handlers::serve_ui)
        .with_state(Arc::new(state))
}
