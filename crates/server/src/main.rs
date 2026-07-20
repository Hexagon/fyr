//! Fyr — Off-Grid Content Platform
//!
//! A self-contained Rust application for offline content distribution and consumption.
//! Supports maps (PMTiles), books (EPUB), and POIs (FlatGeoBuf/GeoJSON).

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue, Method},
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use anyhow::Context;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::cors::CorsLayer;
use types::Config;
use downloader::DownloadManager;
use ai::ModelManager;
use std::fs;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use settings::SettingsManager;

mod ai;
mod auth;
mod handlers;
mod state;
mod settings;

pub use state::AppState;

const MANAGED_MANUALS: [&str; 2] = ["user-manual.md", "developer-manual.md"];

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
    sync_managed_manuals(&config)?;

    if config.auth.readonly {
        info!("Server is running in strict read-only mode (FYR_READONLY)");
    } else if config.auth.admin_password.is_some() {
        info!("Server is running in password-protected admin mode (FYR_ADMIN_PASSWORD)");
    }

    info!("Data directory: {}", config.data_dir.display());
    info!("Server will run on: {}:{}", config.server.host, config.server.port);
    let bind_host = config.server.host.clone();
    let bind_port = config.server.port;
    let config = Arc::new(config);
    let static_path: PathBuf = first_existing_path("public/static", "static");

    info!("Resolved static directory: {}", static_path.display());

    // Create shared application state
    let app_state = AppState {
        config: config.clone(),
        static_dir: static_path,
        download_manager: Arc::new(DownloadManager::new(config.data_dir.clone())),
        model_manager: Arc::new(ModelManager::new(config.clone())),
        settings_manager: Arc::new(SettingsManager::new(config.data_dir.clone())),
        auth_manager: Arc::new(auth::AuthManager::new()),
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
    let static_path: PathBuf = state.static_dir.clone();
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

    // Build the Arc<AppState> early so the auth middleware can capture it.
    let state_arc = Arc::new(state);

    let admin_mw = middleware::from_fn_with_state(
        Arc::clone(&state_arc),
        auth::require_admin,
    );

    // Protected (mutating) routes — guarded by the admin middleware.
    let protected = Router::new()
        .route("/api/download", post(handlers::create_download))
        .route("/api/download/:task_id", delete(handlers::cancel_download))
        .route("/api/download/:task_id/dismiss", delete(handlers::dismiss_download))
        .route("/api/import/download/:filename", post(handlers::create_import_download))
        .route("/api/content/:content_type/:filename", delete(handlers::delete_content_file))
        .route(
            "/api/models/upload",
            post(handlers::ai_upload_model).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/models/import", post(handlers::ai_import_model))
        .route("/api/models/:filename/load", post(handlers::ai_load_model))
        .route("/api/models/:filename/load", delete(handlers::ai_unload_model))
        .route(
            "/api/import/upload",
            post(handlers::upload_file_to_import).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/settings", put(handlers::update_settings))
        .route_layer(admin_mw);

    Router::new()
        // Public read-only API
        .route("/api/status", get(handlers::status))
        .route("/api/config", get(handlers::config))
        .route("/api/settings", get(handlers::get_settings))
        .route("/api/storage", get(handlers::get_storage))
        .route("/api/content/maps", get(handlers::list_maps))
        .route("/api/content/books", get(handlers::list_books))
        .route("/api/content/poi", get(handlers::list_poi))
        .route("/api/content/models", get(handlers::list_models))
        .route("/api/content/misc", get(handlers::list_misc))
        .route("/api/models", get(handlers::ai_list_models))
        .route("/api/models/:filename/health", get(handlers::ai_model_health))
        .route("/api/models/:filename/infer/stream", get(handlers::ai_infer_stream))
        .route("/api/download/:task_id/status", get(handlers::get_download_status))
        .route("/api/downloads", get(handlers::list_downloads))
        .route("/api/reader/capabilities", get(handlers::reader_capabilities))
        .route("/api/reader/open/:filename", get(handlers::reader_open))
        .route("/api/reader/zim/:filename/meta", get(handlers::reader_zim_meta))
        .route("/api/reader/zim/:filename/capabilities", get(handlers::reader_zim_capabilities))
        .route("/api/reader/zim/:filename/native/article", get(handlers::reader_zim_native_article))
        .route("/api/reader/zim/:filename/native/search", get(handlers::reader_zim_native_search))
        .route("/api/reader/zim/:filename/native/content/*path", get(handlers::reader_zim_native_content))
        // Auth endpoints
        .route("/api/auth/status", get(auth::auth_status_handler))
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/auth/logout", post(auth::logout_handler))
        // Protected (admin) routes
        .merge(protected)
        // Data file serving (for PMTiles, etc.) - use configured data directory
        .nest_service("/data", ServeDir::new(data_path))
        // Book-serving alias for URL-based reader integrations
        .nest_service("/docs/books", ServeDir::new(books_path))
        // Optional generic public asset passthrough
        .nest_service("/assets", ServeDir::new(assets_path))
        // Static file serving and SPA fallback
        .nest_service("/static", ServeDir::new(static_path.clone()))
        .fallback(get(handlers::serve_index))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::ACCEPT_RANGES,
            HeaderValue::from_static("bytes"),
        ))
        .layer(cors)
        .with_state(state_arc)
}

fn first_existing_path(primary: &str, fallback: &str) -> PathBuf {
    let candidates = [
        PathBuf::from("/app").join(primary),
        PathBuf::from(primary),
        PathBuf::from("/app").join(fallback),
        PathBuf::from(fallback),
    ];

    for candidate in candidates {
        if Path::new(&candidate).exists() {
            return candidate;
        }
    }

    PathBuf::from(primary)
}

fn sync_managed_manuals(config: &Config) -> anyhow::Result<()> {
    let source_root = [
        PathBuf::from("/app/public/data/books"),
        PathBuf::from("public/data/books"),
        PathBuf::from("./public/data/books"),
    ]
    .into_iter()
    .find(|candidate| candidate.exists());

    let Some(source_root) = source_root else {
        warn!("Skipping managed manual sync: source books directory not found");
        return Ok(());
    };

    let books_dir = config.books_dir();
    fs::create_dir_all(&books_dir).with_context(|| {
        format!(
            "Failed to create books directory for manual sync: {}",
            books_dir.display()
        )
    })?;

    for manual_name in MANAGED_MANUALS {
        let source_path = source_root.join(manual_name);
        if !source_path.exists() {
            warn!(
                "Skipping managed manual sync for {}: source file missing",
                source_path.display()
            );
            continue;
        }

        let target_path = books_dir.join(manual_name);
        if let Err(error) = fs::copy(&source_path, &target_path) {
            if is_file_lock_error(&error) {
                warn!(
                    "Skipping managed manual sync for {} -> {} because the target file is locked: {}",
                    source_path.display(),
                    target_path.display(),
                    error
                );
                continue;
            }

            return Err(error).with_context(|| {
                format!(
                    "Failed to sync managed manual {} -> {}",
                    source_path.display(),
                    target_path.display()
                )
            });
        }

        info!(
            "Synchronized managed manual {} to {}",
            source_path.display(),
            target_path.display()
        );
    }

    Ok(())
}

fn is_file_lock_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32) | Some(33))
}

