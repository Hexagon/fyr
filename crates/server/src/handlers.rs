//! API request/response handlers

use crate::ai::types::{
    ImportModelRequest, ImportModelResponse, InferStreamQuery, LoadModelResponse, ModelHealthResponse,
    UploadModelResponse,
};
use crate::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{sse::{Event, KeepAlive, Sse}, Html},
    Json,
};
use types::{AppSettings, ContentMetadata, ContentType, DownloadSource, GeoPosition};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::Path as FsPath;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use walkdir::WalkDir;
use tracing::{error, warn};

#[derive(Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub status: String,
    pub data_dir: String,
    pub content_count: ContentCountResponse,
}

#[derive(Serialize)]
pub struct ContentCountResponse {
    pub maps: usize,
    pub books: usize,
    pub poi: usize,
    pub models: usize,
    pub misc: usize,
}

#[derive(Serialize)]
pub struct StorageResponse {
    pub data_dir: String,
    pub total_bytes: u64,
    pub total_human: String,
    pub by_category: std::collections::HashMap<String, StorageCategoryInfo>,
}

#[derive(Serialize)]
pub struct StorageCategoryInfo {
    pub bytes: u64,
    pub human: String,
    pub files: usize,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub data_dir: String,
    pub server_host: String,
    pub server_port: u16,
    pub directories: DirectoriesResponse,
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Option<GeoPosition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modules: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Serialize)]
pub struct DirectoriesResponse {
    pub maps: String,
    pub books: String,
    pub poi: String,
    pub inbox: String,
    pub models: String,
    pub misc: String,
}

#[derive(Deserialize)]
pub struct CreateDownloadRequest {
    pub url: String,
}

#[derive(Serialize)]
pub struct CreateDownloadResponse {
    pub task_id: String,
}

#[derive(Serialize)]
pub struct KiwixStatusResponse {
    pub available: bool,
    pub local_url: String,
    pub source_path: String,
}

#[derive(Serialize)]
pub struct KiwixReaderCapabilitiesResponse {
    pub available: bool,
    pub local_url: String,
    pub zim_base_url: String,
    pub supports_direct_http_zim: bool,
    pub supports_http_range: bool,
}

#[derive(Serialize)]
pub struct ErrorMessageResponse {
    pub message: String,
}

/// GET /api/status — Server status and content inventory
pub async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let maps_count = count_files(state.config.maps_dir());
    let books_count = count_files(state.config.books_dir());
    let poi_count = count_files(state.config.poi_dir());
    let models_count = count_files(state.config.models_dir());
    let misc_count = count_files(state.config.misc_dir());

    Json(StatusResponse {
        version: "0.1.0".to_string(),
        status: "running".to_string(),
        data_dir: state.config.data_dir.display().to_string(),
        content_count: ContentCountResponse {
            maps: maps_count,
            books: books_count,
            poi: poi_count,
            models: models_count,
            misc: misc_count,
        },
    })
}

/// GET /api/config — Current configuration
pub async fn config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        data_dir: state.config.data_dir.display().to_string(),
        server_host: state.config.server.host.clone(),
        server_port: state.config.server.port,
        directories: DirectoriesResponse {
            maps: state.config.maps_dir().display().to_string(),
            books: state.config.books_dir().display().to_string(),
            poi: state.config.poi_dir().display().to_string(),
            inbox: state.config.inbox_dir().display().to_string(),
            models: state.config.models_dir().display().to_string(),
            misc: state.config.misc_dir().display().to_string(),
        },
    })
}

/// GET /api/settings — Current persisted application settings
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Json<AppSettings> {
    Json(state.settings_manager.current())
}

/// PUT /api/settings — Replace persisted application settings
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateSettingsRequest>,
) -> Result<Json<AppSettings>, StatusCode> {
    let updated = state
        .settings_manager
        .merge(payload.location, payload.modules)
        .map_err(|error| {
            warn!("Failed to persist settings: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(updated))
}

/// GET /api/storage — Get storage usage information
pub async fn get_storage(State(state): State<Arc<AppState>>) -> Json<StorageResponse> {
    let mut by_category = std::collections::HashMap::new();

    let maps_info = get_dir_size(state.config.maps_dir());
    by_category.insert(
        "maps".to_string(),
        StorageCategoryInfo {
            bytes: maps_info.0,
            human: format_bytes(maps_info.0),
            files: maps_info.1,
        },
    );

    let books_info = get_dir_size(state.config.books_dir());
    by_category.insert(
        "books".to_string(),
        StorageCategoryInfo {
            bytes: books_info.0,
            human: format_bytes(books_info.0),
            files: books_info.1,
        },
    );

    let poi_info = get_dir_size(state.config.poi_dir());
    by_category.insert(
        "poi".to_string(),
        StorageCategoryInfo {
            bytes: poi_info.0,
            human: format_bytes(poi_info.0),
            files: poi_info.1,
        },
    );

    let models_info = get_dir_size(state.config.models_dir());
    by_category.insert(
        "models".to_string(),
        StorageCategoryInfo {
            bytes: models_info.0,
            human: format_bytes(models_info.0),
            files: models_info.1,
        },
    );

    let misc_info = get_dir_size(state.config.misc_dir());
    by_category.insert(
        "misc".to_string(),
        StorageCategoryInfo {
            bytes: misc_info.0,
            human: format_bytes(misc_info.0),
            files: misc_info.1,
        },
    );

    let total_bytes = maps_info.0 + books_info.0 + poi_info.0 + models_info.0 + misc_info.0;

    Json(StorageResponse {
        data_dir: state.config.data_dir.display().to_string(),
        total_bytes,
        total_human: format_bytes(total_bytes),
        by_category,
    })
}

/// GET /api/content/maps — List available maps
pub async fn list_maps(State(state): State<Arc<AppState>>) -> Json<Vec<ContentMetadata>> {
    list_content_files(state.config.maps_dir(), ContentType::Map)
}

/// GET /api/content/books — List available books
pub async fn list_books(State(state): State<Arc<AppState>>) -> Json<Vec<ContentMetadata>> {
    list_content_files(state.config.books_dir(), ContentType::Book)
}

/// GET /api/content/poi — List available POI datasets
pub async fn list_poi(State(state): State<Arc<AppState>>) -> Json<Vec<ContentMetadata>> {
    list_content_files(state.config.poi_dir(), ContentType::Poi)
}

/// GET /api/content/models — List available local GGUF models
pub async fn list_models(State(state): State<Arc<AppState>>) -> Json<Vec<ContentMetadata>> {
    list_content_files(state.config.models_dir(), ContentType::Model)
}

/// GET /api/content/misc — List available generic files
pub async fn list_misc(State(state): State<Arc<AppState>>) -> Json<Vec<ContentMetadata>> {
    list_content_files(state.config.misc_dir(), ContentType::Misc)
}

/// GET /api/models — List model registry entries and load states
pub async fn ai_list_models(State(state): State<Arc<AppState>>) -> Result<Json<Vec<crate::ai::types::ModelRegistryEntry>>, StatusCode> {
    let models = state
        .model_manager
        .list_models()
        .await
        .map_err(|error| {
            error!("Failed to list models: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(models))
}

/// POST /api/models/upload — Upload a GGUF model into inbox before import
pub async fn ai_upload_model(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadModelResponse>), StatusCode> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| {
            warn!("Invalid multipart payload while uploading model: {}", error);
            StatusCode::BAD_REQUEST
        })?
    {
        if field.name() != Some("file") {
            continue;
        }

        let raw_name = field.file_name().ok_or(StatusCode::BAD_REQUEST)?;
        let filename = sanitize_upload_filename(raw_name).ok_or(StatusCode::BAD_REQUEST)?;

        if !filename.to_ascii_lowercase().ends_with(".gguf") {
            return Err(StatusCode::BAD_REQUEST);
        }

        let target_path = state.config.inbox_dir().join(&filename);
        if tokio::fs::try_exists(&target_path)
            .await
            .map_err(|error| {
                error!("Failed to check existing upload target {}: {}", target_path.display(), error);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            tokio::fs::remove_file(&target_path)
                .await
                .map_err(|error| {
                    error!("Failed to remove existing upload target {}: {}", target_path.display(), error);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }

        let mut file = tokio::fs::File::create(&target_path)
            .await
            .map_err(|error| {
                error!("Failed to create upload target {}: {}", target_path.display(), error);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let mut size_bytes = 0u64;
        let mut magic = Vec::with_capacity(4);
        let mut magic_checked = false;

        while let Some(chunk) = field.chunk().await.map_err(|error| {
            warn!("Failed to read upload stream chunk: {}", error);
            StatusCode::BAD_REQUEST
        })? {
            if !magic_checked {
                let needed = 4usize.saturating_sub(magic.len());
                if needed > 0 {
                    magic.extend_from_slice(&chunk[..chunk.len().min(needed)]);
                }
                if magic.len() == 4 {
                    magic_checked = true;
                    if magic.as_slice() != b"GGUF" {
                        let _ = tokio::fs::remove_file(&target_path).await;
                        return Err(StatusCode::UNPROCESSABLE_ENTITY);
                    }
                }
            }

            file.write_all(&chunk)
                .await
                .map_err(|error| {
                    error!("Failed to write upload target {}: {}", target_path.display(), error);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            size_bytes += chunk.len() as u64;
        }

        if magic.len() < 4 {
            let _ = tokio::fs::remove_file(&target_path).await;
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }

        file.flush()
            .await
            .map_err(|error| {
                error!("Failed to flush upload target {}: {}", target_path.display(), error);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        return Ok((
            StatusCode::CREATED,
            Json(UploadModelResponse {
                filename,
                stored_in: target_path.display().to_string(),
                size_bytes,
            }),
        ));
    }

    Err(StatusCode::BAD_REQUEST)
}

/// POST /api/models/import — Import a model from inbox/misc into models with GGUF validation
pub async fn ai_import_model(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportModelRequest>,
) -> Result<Json<ImportModelResponse>, (StatusCode, Json<ErrorMessageResponse>)> {
    let imported_path = state
        .model_manager
        .import_model(&req.filename, &req.source)
        .await
        .map_err(model_error_response)?;

    Ok(Json(ImportModelResponse {
        filename: req.filename,
        imported_to: imported_path.display().to_string(),
    }))
}

/// POST /api/models/:filename/load — Load GGUF model into memory
pub async fn ai_load_model(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Json<LoadModelResponse>, (StatusCode, Json<ErrorMessageResponse>)> {
    state
        .model_manager
        .load_model(&filename)
        .await
        .map_err(model_error_response)?;

    Ok(Json(LoadModelResponse {
        filename,
        state: crate::ai::types::ModelLoadState::Ready,
    }))
}

/// DELETE /api/models/:filename/load — Unload model from memory
pub async fn ai_unload_model(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Json<LoadModelResponse> {
    state.model_manager.unload_model(&filename).await;
    Json(LoadModelResponse {
        filename,
        state: crate::ai::types::ModelLoadState::Unloaded,
    })
}

/// GET /api/models/:filename/health — Get model health state
pub async fn ai_model_health(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Json<ModelHealthResponse> {
    Json(state.model_manager.health(&filename).await)
}

/// GET /api/models/:filename/infer/stream — Token stream via SSE
pub async fn ai_infer_stream(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Query(query): Query<InferStreamQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let temperature = query.temperature.unwrap_or(0.7);
    let max_tokens = query.max_tokens.unwrap_or(256).clamp(1, 4096);

    let rx = state
        .model_manager
        .infer_stream(&filename, query.prompt, temperature, max_tokens)
        .await
        .map_err(|error| map_model_error_to_status(&error))?;

    let token_stream = ReceiverStream::new(rx).map(|token| {
        Ok::<Event, Infallible>(
            Event::default()
                .event("token")
                .data(token),
        )
    });
    let done_stream = tokio_stream::iter(vec![Ok::<Event, Infallible>(Event::default().event("done"))]);
    let stream = token_stream.chain(done_stream);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// POST /api/download — Create a new download task
pub async fn create_download(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDownloadRequest>,
) -> (StatusCode, Json<CreateDownloadResponse>) {
    let source = DownloadSource::Url { url: req.url };
    let task_id = state.download_manager.create_task(source).await;

    (
        StatusCode::CREATED,
        Json(CreateDownloadResponse { task_id }),
    )
}

/// GET /api/download/:task_id/status — Get download task status
pub async fn get_download_status(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<types::DownloadTask>, StatusCode> {
    state
        .download_manager
        .get_task(&task_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

/// GET /api/downloads — List all download tasks
pub async fn list_downloads(State(state): State<Arc<AppState>>) -> Json<Vec<types::DownloadTask>> {
    let tasks = state.download_manager.list_tasks().await;
    Json(tasks)
}

/// DELETE /api/download/:task_id — Cancel a download task
pub async fn cancel_download(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let cancelled = state
        .download_manager
        .cancel_task(&task_id)
        .await
        .map_err(|error| {
            error!("Failed to cancel download task {}: {}", task_id, error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if cancelled {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// GET /api/kiwix/status — Local Kiwix reader availability
pub async fn kiwix_status() -> Json<KiwixStatusResponse> {
    let source_path = if FsPath::new("public/kiwix-static/www/index.html").exists() {
        "public/kiwix-static/www/index.html"
    } else {
        "kiwix-static/www/index.html"
    };
    let available = std::path::Path::new(source_path).exists();

    Json(KiwixStatusResponse {
        available,
        local_url: "/kiwix/www/index.html".to_string(),
        source_path: source_path.to_string(),
    })
}

/// GET /api/reader/kiwix/capabilities — Reader capabilities and local URLs
pub async fn kiwix_reader_capabilities() -> Json<KiwixReaderCapabilitiesResponse> {
    let source_path = if FsPath::new("public/kiwix-static/www/index.html").exists() {
        "public/kiwix-static/www/index.html"
    } else {
        "kiwix-static/www/index.html"
    };
    let available = std::path::Path::new(source_path).exists();

    Json(KiwixReaderCapabilitiesResponse {
        available,
        local_url: "/kiwix/www/index.html".to_string(),
        zim_base_url: "/docs/books".to_string(),
        supports_direct_http_zim: true,
        supports_http_range: true,
    })
}

/// GET / — Fallback handler for SPA (serves index.html)
/// This enables client-side routing in Vue
pub async fn serve_index() -> Html<String> {
    // Read and serve the built index.html file
    let index_path = if FsPath::new("public/static/index.html").exists() {
        "public/static/index.html"
    } else {
        "static/index.html"
    };
    match std::fs::read_to_string(index_path) {
        Ok(content) => Html(content),
        Err(_) => {
            // Fallback HTML if static/index.html doesn't exist
            Html(
                r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Fyr - Off-Grid Content Platform</title>
    <script type="module" src="/static/js/index.js"></script>
    <link rel="stylesheet" href="/static/css/index.css">
</head>
<body>
    <div id="app"></div>
</body>
</html>"#.to_string()
            )
        }
    }
}

// Helper functions

fn count_files(dir: std::path::PathBuf) -> usize {
    if !dir.exists() {
        return 0;
    }
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

fn list_content_files(
    dir: std::path::PathBuf,
    content_type: ContentType,
) -> Json<Vec<ContentMetadata>> {
    if !dir.exists() {
        return Json(Vec::new());
    }

    let files: Vec<ContentMetadata> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.path().to_path_buf();
            let metadata = std::fs::metadata(&path).ok()?;
            Some(ContentMetadata {
                id: path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                name: path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                content_type,
                file_path: path,
                file_size: metadata.len(),
                checksum: None,
                created_at: chrono::Local::now().to_rfc3339(),
            })
        })
        .collect();

    Json(files)
}

fn get_dir_size(dir: std::path::PathBuf) -> (u64, usize) {
    if !dir.exists() {
        return (0, 0);
    }

    let mut total_size = 0u64;
    let mut file_count = 0usize;

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(metadata) = std::fs::metadata(entry.path()) {
            total_size += metadata.len();
            file_count += 1;
        }
    }

    (total_size, file_count)
}

fn format_bytes(bytes: u64) -> String {
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut index = 0;

    while size >= 1024.0 && index < sizes.len() - 1 {
        size /= 1024.0;
        index += 1;
    }

    format!("{:.2} {}", size, sizes[index])
}

fn map_model_error_to_status(error: &crate::ai::error::ModelError) -> StatusCode {
    use crate::ai::error::ModelError;
    match error {
        ModelError::NotFound(_) => StatusCode::NOT_FOUND,
        ModelError::InvalidExtension(_) => StatusCode::BAD_REQUEST,
        ModelError::InvalidMagic(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ModelError::GgufParse(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ModelError::MissingTokenizerMetadata(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ModelError::NotLoaded(_) => StatusCode::CONFLICT,
        ModelError::ImportFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        ModelError::InferenceFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        ModelError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn model_error_response(error: crate::ai::error::ModelError) -> (StatusCode, Json<ErrorMessageResponse>) {
    let status = map_model_error_to_status(&error);
    (
        status,
        Json(ErrorMessageResponse {
            message: user_facing_model_error(&error),
        }),
    )
}

fn user_facing_model_error(error: &crate::ai::error::ModelError) -> String {
    let message = error.to_string();

    if message.contains("unknown dtype") {
        return "Unsupported GGUF tensor format in this model. It likely needs a newer runtime than the current Candle backend.".to_string();
    }

    if let Some(first_line) = message.lines().next() {
        return first_line.trim().to_string();
    }

    message
}

fn sanitize_upload_filename(filename: &str) -> Option<String> {
    let candidate = std::path::Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())?
        .trim();

    if candidate.is_empty() {
        return None;
    }

    Some(candidate.to_string())
}
