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
use tower_http::services::{ServeDir, ServeFile};
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
mod library;
mod mbtiles;
mod messaging;
mod search;
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
    configure_ai_runtime(&config);

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

    let settings_manager = Arc::new(SettingsManager::new(config.data_dir.clone()));
    let download_manager = Arc::new(DownloadManager::new(config.data_dir.clone()));

    let initial_settings = settings_manager.current();
    let initial_download_timeout = handlers::resolve_download_request_timeout_secs(&initial_settings);
    download_manager
        .set_request_timeout_secs(initial_download_timeout)
        .await;

    let shutdown_downloads = Arc::clone(&download_manager);

    // Create shared application state
    let app_state = AppState {
        config: config.clone(),
        static_dir: static_path,
        download_manager,
        model_manager: Arc::new(ModelManager::new(config.clone())),
        settings_manager,
        auth_manager: Arc::new(auth::AuthManager::new()),
        mbtiles_pool: Arc::new(mbtiles::MbtilesPool::new()),
        messaging: Arc::new(messaging::Messaging::open(&config)?),
    };
    if app_state.messaging.enabled() {
        app_state.messaging.spawn_pruner();
    } else {
        info!("Messages are disabled (FYR_MESSAGING=false)");
    }

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

    // Graceful shutdown: on SIGINT/SIGTERM, first stop download workers and
    // persist their byte counts (they become resumable), then let axum drain
    // open connections for a bounded time. Long-lived responses (SSE, large
    // file transfers) must not keep the process alive indefinitely.
    let (drain_started_tx, drain_started_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        info!("Shutdown requested; persisting download state");
        shutdown_downloads.shutdown(DOWNLOAD_SHUTDOWN_GRACE).await;
        let _ = drain_started_tx.send(());
    });

    tokio::select! {
        result = std::future::IntoFuture::into_future(server) => result?,
        _ = async {
            if drain_started_rx.await.is_ok() {
                tokio::time::sleep(CONNECTION_DRAIN_TIMEOUT).await;
            } else {
                std::future::pending::<()>().await;
            }
        } => {
            warn!("Open connections did not close within {:?}; exiting", CONNECTION_DRAIN_TIMEOUT);
        }
    }

    info!("Fyr stopped");
    Ok(())
}

/// How long download workers get to stop and record their partial files.
const DOWNLOAD_SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(3);
/// How long open HTTP connections may drain after shutdown starts.
const CONNECTION_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Resolves on Ctrl+C (all platforms) or SIGTERM (Unix, e.g. `docker stop`).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            warn!("Failed to listen for Ctrl+C: {}", error);
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                warn!("Failed to listen for SIGTERM: {}", error);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Create the API router with all endpoints
fn create_router(state: AppState) -> Router {
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
        .route("/api/download/:task_id/retry", post(handlers::retry_download))
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
        .route("/api/poi/:filename", put(handlers::save_poi_file));
    // Messaging moderation (delete/pin message, delete channel) is admin-only.
    let protected = if state_arc.messaging.enabled() {
        protected.merge(messaging::routes::moderation_routes())
    } else {
        protected
    }
    .route_layer(admin_mw);

    // Messaging writes (post message, create channel): an explicit,
    // narrowly-scoped exception to "mutations are admin-only". Guarded by
    // `require_messaging_write` (read-only → 403; password mode → admin
    // session or FYR_GUEST_MESSAGING) plus per-IP rate limits in handlers.
    let messaging_write = if state_arc.messaging.enabled() {
        messaging::routes::write_routes()
            .layer(DefaultBodyLimit::max(messaging::MAX_BODY_BYTES))
            .route_layer(middleware::from_fn_with_state(
                Arc::clone(&state_arc),
                auth::require_messaging_write,
            ))
    } else {
        Router::new()
    };
    let messaging_public = if state_arc.messaging.enabled() {
        messaging::routes::public_routes()
    } else {
        Router::new().route("/api/messages/config", get(messaging::routes::config_handler))
    };

    Router::new()
        // Public read-only API
        .route("/api/status", get(handlers::status))
        .route("/api/config", get(handlers::config))
        .route("/api/settings", get(handlers::get_settings))
        .route("/api/storage", get(handlers::get_storage))
        .route("/api/content/maps", get(handlers::list_maps))
        .route("/api/maps/tiles/:filename/metadata", get(handlers::serve_mbtiles_metadata))
        .route("/api/maps/tiles/:filename/:z/:x/:y", get(handlers::serve_mbtile))
        .route("/api/content/books", get(handlers::list_books))
        .route("/api/content/poi", get(handlers::list_poi))
        .route("/api/content/models", get(handlers::list_models))
        .route("/api/content/misc", get(handlers::list_misc))
        .route(
            "/api/content/:content_type/:filename/download",
            get(handlers::download_content_file),
        )
        .route("/api/models", get(handlers::ai_list_models))
        .route("/api/models/:filename/health", get(handlers::ai_model_health))
        .route(
            "/api/models/:filename/infer/stream",
            get(handlers::ai_infer_stream).post(handlers::ai_infer_stream_post),
        )
        .route("/api/download/:task_id/status", get(handlers::get_download_status))
        .route("/api/downloads", get(handlers::list_downloads))
        .route("/api/reader/capabilities", get(handlers::reader_capabilities))
        .route("/api/reader/open/:filename", get(handlers::reader_open))
        .route("/api/reader/zim/:filename/meta", get(handlers::reader_zim_meta))
        .route("/api/reader/zim/:filename/capabilities", get(handlers::reader_zim_capabilities))
        .route("/api/reader/zim/:filename/native/article", get(handlers::reader_zim_native_article))
        .route("/api/reader/zim/:filename/native/search", get(handlers::reader_zim_native_search))
        .route("/api/reader/zim/:filename/native/content/*path", get(handlers::reader_zim_native_content))
        // Unified library API (read-only book metadata, TOC, search)
        .route("/api/library/books/:filename", get(handlers::library_book_metadata))
        .route("/api/library/books/:filename/toc", get(handlers::library_book_toc))
        .route("/api/library/books/:filename/search", get(handlers::library_book_search))
        // Global offline search across books, ZIM articles, maps and POIs (read-only)
        .route("/api/search", get(search::global_search))
        // Tools endpoints (read-only computation, no state mutation)
        .route("/api/tools/aes", post(handlers::tools_aes))
        .route("/api/tools/hash", post(handlers::tools_hash))
        // Auth endpoints
        .route("/api/auth/status", get(auth::auth_status_handler))
        .route("/api/auth/login", post(auth::login_handler))
        .route("/api/auth/logout", post(auth::logout_handler))
        // Messages: public reads/streams, guarded writes
        .merge(messaging_public)
        .merge(messaging_write)
        // Protected (admin) routes
        .merge(protected)
        // Data file serving (PMTiles, books, POI, curated catalog). Only the
        // public content folders are exposed; settings.json,
        // download_tasks.json, inbox/, models/ and misc/ stay private.
        .nest_service("/data/maps", ServeDir::new(state_arc.config.maps_dir()))
        .nest_service("/data/books", ServeDir::new(books_path.clone()))
        .nest_service("/data/poi", ServeDir::new(state_arc.config.poi_dir()))
        .route_service(
            "/data/curated-content.json",
            ServeFile::new(state_arc.config.curated_content_path()),
        )
        // Book-serving alias for URL-based reader integrations
        .nest_service("/docs/books", ServeDir::new(books_path))
        // Optional generic public asset passthrough
        .nest_service("/assets", ServeDir::new(assets_path))
        // Static file serving and SPA fallback
        .merge(
            Router::new()
                .nest_service("/static", ServeDir::new(static_path.clone()))
                .layer(middleware::from_fn(static_cache_headers)),
        )
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

/// Vite emits content-hashed bundles under `js/` and `css/`, so they can be
/// cached forever; a new build produces new filenames referenced by the
/// (never cached) `index.html`. Everything else under `/static` keeps the
/// global `no-store` policy.
async fn static_cache_headers(
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path();
    let is_hashed_bundle = path.starts_with("/static/js/") || path.starts_with("/static/css/");
    let mut response = next.run(request).await;
    if is_hashed_bundle && (response.status().is_success() || response.status() == axum::http::StatusCode::NOT_MODIFIED) {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

/// Log detected CPU SIMD capabilities and size Candle's CPU thread pool.
///
/// Runs on the real deployment hardware at process startup, not at Docker
/// build time, so the logged feature status is accurate even when the
/// image itself was built under QEMU emulation for a foreign architecture.
fn configure_ai_runtime(config: &Config) {
    log_cpu_feature_support();

    let candle_threads_set = std::env::var("CANDLE_NUM_THREADS").is_ok();
    let rayon_threads_set = std::env::var("RAYON_NUM_THREADS").is_ok();
    if candle_threads_set || rayon_threads_set {
        info!("Candle CPU thread pool size is controlled by an existing CANDLE_NUM_THREADS/RAYON_NUM_THREADS environment variable; leaving it as-is");
        return;
    }

    let threads = config.ai.threads.unwrap_or_else(default_ai_thread_count).max(1);

    info!(
        "Configuring Candle CPU inference thread pool: {} thread(s) (override with FYR_AI_THREADS, or CANDLE_NUM_THREADS/RAYON_NUM_THREADS directly)",
        threads
    );

    // SAFETY: runs synchronously, very early in `main`, before the Tokio
    // runtime schedules any tasks that read process environment variables
    // and before Candle's lazily-initialized thread pool is touched by any
    // inference call. No other thread can observe a torn read at this
    // point in startup.
    unsafe {
        std::env::set_var("CANDLE_NUM_THREADS", threads.to_string());
        std::env::set_var("RAYON_NUM_THREADS", threads.to_string());
    }
}

/// Container/cgroup-aware default thread count for CPU inference.
///
/// `available_parallelism()` respects cgroup CPU quotas on Linux, unlike a
/// naive core count, which matters for Docker deployments given a
/// fractional CPU allowance.
fn default_ai_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn log_cpu_feature_support() {
    #[cfg(target_arch = "aarch64")]
    {
        let compiled_neon = cfg!(target_feature = "neon");
        let compiled_dotprod = cfg!(target_feature = "dotprod");
        let runtime_neon = std::arch::is_aarch64_feature_detected!("neon");
        let runtime_fp16 = std::arch::is_aarch64_feature_detected!("fp16");
        let runtime_dotprod = std::arch::is_aarch64_feature_detected!("dotprod");
        let runtime_fcma = std::arch::is_aarch64_feature_detected!("fcma");

        info!(
            "CPU features (aarch64): compiled[neon={} dotprod={}] runtime[neon={} fp16={} dotprod={} fcma={}]",
            compiled_neon, compiled_dotprod, runtime_neon, runtime_fp16, runtime_dotprod, runtime_fcma
        );

        if runtime_dotprod && !compiled_dotprod {
            info!("This CPU supports the 'dotprod' instruction (e.g. Raspberry Pi 5 / Cortex-A75 and newer), but this binary was not compiled with it, so quantized inference is using a slower fallback kernel. Rebuild the Docker image with --build-arg RUST_TARGET_FEATURES=+dotprod for faster inference on this hardware. Do not use this flag for images that must also run on older boards such as Raspberry Pi 3/4.");
        }
    }

    #[cfg(target_arch = "x86_64")]
    {
        let compiled_avx2 = cfg!(target_feature = "avx2");
        let compiled_fma = cfg!(target_feature = "fma");
        let runtime_avx2 = std::arch::is_x86_feature_detected!("avx2");
        let runtime_fma = std::arch::is_x86_feature_detected!("fma");

        info!(
            "CPU features (x86_64): compiled[avx2={} fma={}] runtime[avx2={} fma={}]",
            compiled_avx2,
            compiled_fma,
            runtime_avx2,
            runtime_fma,
        );

        if runtime_avx2 && runtime_fma && (!compiled_avx2 || !compiled_fma) {
            info!("This CPU supports AVX2/FMA, but this binary was not compiled with those features, so quantized inference may run slower. For x86_64 self-builds on known-compatible hardware, rebuild with --build-arg RUST_TARGET_FEATURES=+avx2,+fma. Do not use this flag for images that must also run on older x86_64 CPUs without AVX2/FMA.");
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let data_dir = std::env::temp_dir().join(format!("fyr-router-tests-{}", uuid::Uuid::new_v4()));
        let config = Config::default_with_data_dir(&data_dir);
        config.initialize_directories().expect("initialize directories");
        let config = Arc::new(config);

        AppState {
            config: Arc::clone(&config),
            static_dir: data_dir.join("static"),
            download_manager: Arc::new(DownloadManager::new(&data_dir)),
            model_manager: Arc::new(ModelManager::new(Arc::clone(&config))),
            settings_manager: Arc::new(SettingsManager::new(&data_dir)),
            auth_manager: Arc::new(auth::AuthManager::new()),
            mbtiles_pool: Arc::new(mbtiles::MbtilesPool::new()),
            messaging: Arc::new(messaging::Messaging::open(&config).expect("open messages db")),
        }
    }

    async fn get(app: &Router, uri: &str, range: Option<&str>) -> axum::response::Response {
        let mut request = Request::get(uri);
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        app.clone()
            .oneshot(request.body(Body::empty()).expect("request"))
            .await
            .expect("response")
    }

    async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body")
            .to_vec()
    }

    #[tokio::test]
    async fn data_mount_serves_public_content_only() {
        let state = test_state();
        let data_dir = state.config.data_dir.clone();
        fs::write(state.config.books_dir().join("guide.md"), "# Guide").expect("write book");
        fs::write(state.config.poi_dir().join("camps.geojson"), "{}").expect("write poi");
        fs::write(data_dir.join("settings.json"), "{}").expect("write settings");
        fs::write(data_dir.join("download_tasks.json"), "{}").expect("write tasks");
        fs::write(state.config.inbox_dir().join("staged.epub"), "x").expect("write inbox");
        fs::write(state.config.models_dir().join("model.gguf"), "GGUF").expect("write model");
        let app = create_router(state);

        for public in [
            "/data/books/guide.md",
            "/data/poi/camps.geojson",
            "/data/curated-content.json",
        ] {
            assert_eq!(get(&app, public, None).await.status(), StatusCode::OK, "{public}");
        }

        for private in [
            "/data/settings.json",
            "/data/download_tasks.json",
            "/data/inbox/staged.epub",
            "/data/models/model.gguf",
            "/data/maps/../settings.json",
            "/api/does-not-exist",
        ] {
            assert_eq!(get(&app, private, None).await.status(), StatusCode::NOT_FOUND, "{private}");
        }

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn mbtiles_tiles_are_served_from_pooled_connections() {
        let state = test_state();
        let data_dir = state.config.data_dir.clone();
        let archive = state.config.maps_dir().join("region.mbtiles");
        {
            let conn = rusqlite::Connection::open(&archive).expect("create archive");
            conn.execute_batch(
                "CREATE TABLE metadata (name TEXT, value TEXT);
                 CREATE TABLE tiles (zoom_level INTEGER, tile_column INTEGER, tile_row INTEGER, tile_data BLOB);
                 INSERT INTO metadata VALUES ('format', 'png');",
            )
            .expect("create schema");
            // XYZ z=1 x=1 y=0 is TMS row 1.
            conn.execute(
                "INSERT INTO tiles VALUES (1, 1, 1, ?1)",
                [b"\x89PNG-tile".to_vec()],
            )
            .expect("insert tile");
        }
        let app = create_router(state);

        for _ in 0..3 {
            let tile = get(&app, "/api/maps/tiles/region.mbtiles/1/1/0", None).await;
            assert_eq!(tile.status(), StatusCode::OK);
            assert_eq!(tile.headers()[header::CONTENT_TYPE], "image/png");
            assert_eq!(body_bytes(tile).await, b"\x89PNG-tile".to_vec());
        }

        let missing = get(&app, "/api/maps/tiles/region.mbtiles/1/0/0", None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let out_of_range = get(&app, "/api/maps/tiles/region.mbtiles/1/5/0", None).await;
        assert_eq!(out_of_range.status(), StatusCode::NOT_FOUND);

        let metadata = get(&app, "/api/maps/tiles/region.mbtiles/metadata", None).await;
        assert_eq!(metadata.status(), StatusCode::OK);
        assert_eq!(body_bytes(metadata).await, br#"{"format":"png"}"#.to_vec());

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn hashed_static_bundles_are_cached_immutably() {
        let mut state = test_state();
        let static_dir = state.config.data_dir.join("static");
        fs::create_dir_all(static_dir.join("js")).expect("create js dir");
        fs::create_dir_all(static_dir.join("assets")).expect("create assets dir");
        fs::write(static_dir.join("js/index-abc123.js"), "console.log(1)").expect("write bundle");
        fs::write(static_dir.join("assets/fyr-icon.svg"), "<svg/>").expect("write icon");
        state.static_dir = static_dir;
        let data_dir = state.config.data_dir.clone();
        let app = create_router(state);

        let bundle = get(&app, "/static/js/index-abc123.js", None).await;
        assert_eq!(bundle.status(), StatusCode::OK);
        assert_eq!(bundle.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");

        let icon = get(&app, "/static/assets/fyr-icon.svg", None).await;
        assert_eq!(icon.status(), StatusCode::OK);
        assert!(icon.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));

        let missing = get(&app, "/static/js/missing.js", None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert!(missing.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn infer_stream_on_unloaded_model_is_conflict_with_or_without_library() {
        let state = test_state();
        let data_dir = state.config.data_dir.clone();
        fs::write(
            state.config.books_dir().join("guide.md"),
            "# Guide\n\nBoil water for one minute before drinking.\n",
        )
        .expect("write book");
        let app = create_router(state);

        for uri in [
            "/api/models/missing.gguf/infer/stream?prompt=boil%20water",
            "/api/models/missing.gguf/infer/stream?prompt=boil%20water&use_library=false",
            "/api/models/missing.gguf/infer/stream?prompt=boil%20water&use_library=true",
            "/api/models/missing.gguf/infer/stream?prompt=boil%20water&use_library=true&sources_limit=99",
        ] {
            let response = get(&app, uri, None).await;
            assert_eq!(response.status(), StatusCode::CONFLICT, "{uri}");
        }

        let bad_flag = get(&app, "/api/models/missing.gguf/infer/stream?prompt=x&use_library=maybe", None).await;
        assert_eq!(bad_flag.status(), StatusCode::BAD_REQUEST);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn infer_stream_post_accepts_long_history_and_stays_public() {
        let mut state = test_state();
        let data_dir = state.config.data_dir.clone();
        let mut config = (*state.config).clone();
        config.auth.readonly = true;
        state.config = Arc::new(config);
        let app = create_router(state);

        // ~300 KB of history: far beyond what fits in a URL.
        let history: Vec<serde_json::Value> = (0..300)
            .map(|i| {
                serde_json::json!({
                    "role": if i % 2 == 0 { "user" } else { "assistant" },
                    "text": "x".repeat(1000),
                })
            })
            .collect();
        let body = serde_json::json!({
            "prompt": "boil water",
            "history": history,
            "use_library": true,
        })
        .to_string();

        // Inference changes no state, so read-only mode does not block it;
        // the unloaded model is the only reason it fails.
        let response = post_json(&app, "/api/models/missing.gguf/infer/stream", &body, None).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let malformed = post_json(&app, "/api/models/missing.gguf/infer/stream", "{\"history\": 1}", None).await;
        assert!(malformed.status().is_client_error(), "{}", malformed.status());

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn restricted_mode_hides_host_paths_from_anonymous_callers() {
        let mut state = test_state();
        let data_dir = state.config.data_dir.clone();
        fs::write(state.config.maps_dir().join("region.pmtiles"), "PMTiles").expect("write map");
        let mut config = (*state.config).clone();
        config.auth.admin_password = Some("secret".to_string());
        state.config = Arc::new(config);
        let token = state.auth_manager.create_session();
        let app = create_router(state);
        let data_dir_str = data_dir.display().to_string();

        for uri in ["/api/status", "/api/config", "/api/storage", "/api/content/maps"] {
            let anonymous = String::from_utf8(body_bytes(get(&app, uri, None).await).await).unwrap();
            assert!(!anonymous.contains(&data_dir_str), "{uri} leaked a host path: {anonymous}");

            let admin = app
                .clone()
                .oneshot(
                    Request::get(uri)
                        .header(header::COOKIE, format!("fyr_session={token}"))
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("response");
            let admin = String::from_utf8(body_bytes(admin).await).unwrap();
            assert!(admin.contains(&data_dir_str), "{uri} should show paths to admins: {admin}");
        }

        let maps = String::from_utf8(body_bytes(get(&app, "/api/content/maps", None).await).await).unwrap();
        assert!(maps.contains(r#""file_path":"region.pmtiles""#), "{maps}");

        let _ = fs::remove_dir_all(data_dir);
    }

    // --- Download retry endpoint -------------------------------------------

    async fn post_json(app: &Router, uri: &str, body: &str, cookie: Option<&str>) -> axum::response::Response {
        let mut request = Request::post(uri).header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = cookie {
            request = request.header(header::COOKIE, format!("fyr_session={token}"));
        }
        app.clone()
            .oneshot(request.body(Body::from(body.to_string())).expect("request"))
            .await
            .expect("response")
    }

    #[tokio::test]
    async fn download_retry_endpoint_requires_admin() {
        let mut state = test_state();
        let data_dir = state.config.data_dir.clone();
        let mut config = (*state.config).clone();
        config.auth.admin_password = Some("secret".to_string());
        state.config = Arc::new(config);
        let token = state.auth_manager.create_session();
        let app = create_router(state);

        let anonymous = post_json(&app, "/api/download/some-task/retry", "", None).await;
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

        // With a session the middleware lets it through to the handler.
        let admin = post_json(&app, "/api/download/some-task/retry", "", Some(&token)).await;
        assert_eq!(admin.status(), StatusCode::NOT_FOUND);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn download_retry_endpoint_is_blocked_in_readonly_mode() {
        let mut state = test_state();
        let data_dir = state.config.data_dir.clone();
        let mut config = (*state.config).clone();
        config.auth.readonly = true;
        state.config = Arc::new(config);
        let app = create_router(state);

        let response = post_json(&app, "/api/download/some-task/retry", "", None).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn create_download_validates_sha256() {
        let state = test_state();
        let data_dir = state.config.data_dir.clone();
        let app = create_router(state);

        let bad = post_json(&app, "/api/download", r#"{"url":"http://127.0.0.1:9/x.zip","sha256":"abc"}"#, None).await;
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        let message = String::from_utf8(body_bytes(bad).await).unwrap();
        assert!(message.contains("sha256"), "{message}");

        let good = post_json(
            &app,
            "/api/download",
            &format!(r#"{{"url":"http://127.0.0.1:9/x.zip","sha256":"{}"}}"#, "A".repeat(64)),
            None,
        )
        .await;
        assert_eq!(good.status(), StatusCode::CREATED);

        let _ = fs::remove_dir_all(data_dir);
    }

    // --- End download retry endpoint ---------------------------------------

    #[tokio::test]
    async fn content_download_streams_with_filename_and_range_support() {
        let state = test_state();
        let data_dir = state.config.data_dir.clone();
        let payload: Vec<u8> = (0..=255u8).cycle().take(100_000).collect();
        fs::write(state.config.misc_dir().join("field notes.zip"), &payload).expect("write misc");
        let app = create_router(state);

        let full = get(&app, "/api/content/misc/field%20notes.zip/download", None).await;
        assert_eq!(full.status(), StatusCode::OK);
        assert_eq!(full.headers()[header::CONTENT_LENGTH], "100000");
        assert_eq!(full.headers()[header::CONTENT_TYPE], "application/octet-stream");
        let disposition = full.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().to_string();
        assert!(disposition.starts_with("attachment; filename=\"field notes.zip\""), "{disposition}");
        assert!(disposition.contains("filename*=UTF-8''field%20notes.zip"), "{disposition}");
        assert_eq!(body_bytes(full).await, payload);

        let partial = get(&app, "/api/content/misc/field%20notes.zip/download", Some("bytes=10-19")).await;
        assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 10-19/100000");
        assert_eq!(body_bytes(partial).await, payload[10..20].to_vec());

        let missing = get(&app, "/api/content/misc/absent.zip/download", None).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let _ = fs::remove_dir_all(data_dir);
    }
    // --- Global search (/api/search) ---------------------------------------
    #[tokio::test]
    async fn global_search_aggregates_sources_without_leaking_paths() {
        let mut state = test_state();
        let data_dir = state.config.data_dir.clone();
        fs::write(
            state.config.books_dir().join("alpine-guide.md"),
            "# Alpine Survival Guide\n\nBoil water from alpine streams.\n",
        )
        .expect("write book");
        fs::write(state.config.maps_dir().join("alpine_region.pmtiles"), "PMTiles").expect("write map");
        fs::write(
            state.config.poi_dir().join("huts.geojson"),
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[7.5,46.5]},"properties":{"name":"Alpine Hut"}}]}"#,
        )
        .expect("write poi");
        fs::write(state.config.poi_dir().join("broken.geojson"), "{ not json").expect("write broken poi");
        // Strict read-only mode: search is a public read-only endpoint.
        let mut config = (*state.config).clone();
        config.auth.readonly = true;
        state.config = Arc::new(config);
        let app = create_router(state);
        let data_dir_str = data_dir.display().to_string();

        let response = get(&app, "/api/search?q=Alpine", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        let text = String::from_utf8(body_bytes(response).await).unwrap();
        assert!(!text.contains(&data_dir_str), "search leaked a host path: {text}");
        let body: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(body["query"], "Alpine");
        let results = body["results"].as_array().expect("results");
        let link_of = |kind: &str| {
            results
                .iter()
                .find(|r| r["kind"] == kind)
                .and_then(|r| r["link"].as_str())
                .unwrap_or_else(|| panic!("missing {kind} result in {text}"))
                .to_string()
        };
        assert_eq!(link_of("book"), "/books?file=alpine-guide.md");
        assert_eq!(link_of("map"), "/maps?map=alpine_region.pmtiles");
        assert_eq!(link_of("poi"), "/maps?lat=46.5&lng=7.5&zoom=14&label=Alpine%20Hut");
        assert_eq!(link_of("content"), "/books?file=alpine-guide.md&q=Alpine");
        for result in results {
            assert!(result["source"].as_str().is_some_and(|s| !s.contains('/')), "{result}");
            assert!(result["score"].is_number());
            assert!(result["title"].is_string());
        }
        let book = results.iter().find(|r| r["kind"] == "book").unwrap();
        assert_eq!(book["title"], "Alpine Survival Guide");

        // kinds filter + limit
        let maps_only: serde_json::Value = serde_json::from_slice(
            &body_bytes(get(&app, "/api/search?q=alpine&kinds=map&limit=5", None).await).await,
        )
        .unwrap();
        let maps_only = maps_only["results"].as_array().unwrap();
        assert_eq!(maps_only.len(), 1);
        assert_eq!(maps_only[0]["kind"], "map");

        let limited: serde_json::Value = serde_json::from_slice(
            &body_bytes(get(&app, "/api/search?q=alpine&limit=1", None).await).await,
        )
        .unwrap();
        assert_eq!(limited["results"].as_array().unwrap().len(), 1);
        assert_eq!(limited["truncated"], true);

        // Empty, missing and huge queries are fine.
        for uri in ["/api/search", "/api/search?q=", "/api/search?q=%20%20"] {
            let response = get(&app, uri, None).await;
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            let body: serde_json::Value = serde_json::from_slice(&body_bytes(response).await).unwrap();
            assert!(body["results"].as_array().unwrap().is_empty(), "{uri}");
        }
        let huge = format!("/api/search?q={}", "x".repeat(5000));
        assert_eq!(get(&app, &huge, None).await.status(), StatusCode::OK);

        let _ = fs::remove_dir_all(data_dir);
    }
}
