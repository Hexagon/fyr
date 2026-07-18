//! Fyr — Off-Grid Content Platform
//!
//! A self-contained Rust application for offline content distribution and consumption.
//! Supports maps (PMTiles), books (EPUB), and POIs (FlatGeoBuf/GeoJSON).

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue, Method},
    routing::{delete, get, post},
    Router,
};
use anyhow::Context;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::cors::CorsLayer;
use types::Config;
use downloader::DownloadManager;
use ai::ModelManager;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tracing::info;
use settings::SettingsManager;

mod ai;
mod handlers;
mod state;
mod settings;

pub use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Fyr");

    // Initialize configuration
    let config = Config::default();
    config.initialize_directories()?;
    config.validate_writable()?;

    info!("Data directory: {}", config.data_dir.display());
    info!("Server will run on: {}:{}", config.server.host, config.server.port);
    let bind_host = config.server.host.clone();
    let bind_port = config.server.port;
    let config = Arc::new(config);

    // Create shared application state
    let app_state = AppState {
        config: config.clone(),
        download_manager: Arc::new(DownloadManager::new(config.data_dir.clone())),
        model_manager: Arc::new(ModelManager::new(config.clone())),
        settings_manager: Arc::new(SettingsManager::new(config.data_dir.clone())),
    };

    // Build router
    let app = create_router(app_state);

    // Run server
    let bind_addr = format!("{}:{}", bind_host, bind_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| {
            format!(
                "Failed to bind server to {}. Check FYR_HOST/FYR_PORT and ensure the port is free.",
                bind_addr
            )
        })?;

    info!("Server listening on http://{}:{}", bind_host, bind_port);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Create the API router with all endpoints
fn create_router(state: AppState) -> Router {
    let data_path: PathBuf = state.config.data_dir.clone();
    let books_path: PathBuf = state.config.books_dir();
    let static_path: PathBuf = first_existing_path("public/static", "static");
    let kiwix_path: PathBuf = first_existing_path("public/kiwix-static", "kiwix-static");
    let assets_path: PathBuf = first_existing_path("public/assets", "assets");
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://127.0.0.1:8080"),
            HeaderValue::from_static("http://localhost:8080"),
        ])
        .allow_methods([Method::GET, Method::HEAD, Method::OPTIONS])
        .allow_headers([
            header::RANGE,
            header::ACCEPT,
            header::CONTENT_TYPE,
        ])
        .expose_headers([
            header::CONTENT_TYPE,
            header::CONTENT_LENGTH,
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
        ]);
    
    Router::new()
        // API routes
        .route("/api/status", get(handlers::status))
        .route("/api/config", get(handlers::config))
        .route("/api/settings", get(handlers::get_settings).put(handlers::update_settings))
        .route("/api/storage", get(handlers::get_storage))
        .route("/api/content/maps", get(handlers::list_maps))
        .route("/api/content/books", get(handlers::list_books))
        .route("/api/content/poi", get(handlers::list_poi))
        .route("/api/content/models", get(handlers::list_models))
        .route("/api/content/misc", get(handlers::list_misc))
        .route("/api/models", get(handlers::ai_list_models))
        .route(
            "/api/models/upload",
            post(handlers::ai_upload_model).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/models/import", post(handlers::ai_import_model))
        .route("/api/models/:filename/load", post(handlers::ai_load_model))
        .route("/api/models/:filename/load", delete(handlers::ai_unload_model))
        .route("/api/models/:filename/health", get(handlers::ai_model_health))
        .route("/api/models/:filename/infer/stream", get(handlers::ai_infer_stream))
        .route("/api/download", post(handlers::create_download))
        .route("/api/download/:task_id", delete(handlers::cancel_download))
        .route("/api/download/:task_id/status", get(handlers::get_download_status))
        .route("/api/downloads", get(handlers::list_downloads))
        .route("/api/kiwix/status", get(handlers::kiwix_status))
        .route("/api/reader/kiwix/capabilities", get(handlers::kiwix_reader_capabilities))
        // Data file serving (for PMTiles, etc.) - use configured data directory
        .nest_service("/data", ServeDir::new(data_path))
        // Book-serving alias for URL-based reader integrations
        .nest_service("/docs/books", ServeDir::new(books_path))
        // Local Kiwix static reader bundle (kept outside Vite output dir)
        .nest_service("/kiwix", ServeDir::new(kiwix_path))
        // Optional generic public asset passthrough
        .nest_service("/assets", ServeDir::new(assets_path))
        // Static file serving and SPA fallback
        .nest_service("/static", ServeDir::new(static_path.clone()))
        .fallback_service(ServeDir::new(static_path).fallback(get(handlers::serve_index)))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::ACCEPT_RANGES,
            HeaderValue::from_static("bytes"),
        ))
        .layer(cors)
        .with_state(Arc::new(state))
}

fn first_existing_path(primary: &str, fallback: &str) -> PathBuf {
    if Path::new(primary).exists() {
        PathBuf::from(primary)
    } else {
        PathBuf::from(fallback)
    }
}
