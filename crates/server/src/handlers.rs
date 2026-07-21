//! API request/response handlers

use crate::ai::types::{
    ImportModelRequest, ImportModelResponse, InferStreamQuery, LoadModelResponse,
    ModelHealthResponse, UploadModelResponse,
};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Response,
    },
    Json,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::panic::AssertUnwindSafe;
use std::path::Path as FsPath;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{error, warn};
use types::{AppSettings, ContentMetadata, ContentType, DownloadSource, GeoPosition};
use walkdir::WalkDir;
use zim::{DirectoryEntry, MimeType, Namespace, Zim};

const DEFAULT_ASSISTANT_TEMPERATURE: f64 = 0.2;
const DEFAULT_ASSISTANT_MAX_TOKENS: usize = 512;
const DEFAULT_ASSISTANT_NUM_CTX: usize = 2048;
const HIGH_RAM_ASSISTANT_NUM_CTX: usize = 8192;
const HIGH_RAM_THRESHOLD_KIB: u64 = 16 * 1024 * 1024;

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
    pub free_bytes: Option<u64>,
    pub free_human: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub capacity_human: Option<String>,
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
pub struct UploadFileResponse {
    pub filename: String,
    pub stored_in: String,
    pub size_bytes: u64,
    pub detected_type: Option<ContentType>,
}

#[derive(Serialize)]
pub struct ErrorMessageResponse {
    pub message: String,
}

#[derive(Serialize)]
pub struct ReaderFormatCapabilities {
    pub format: String,
    pub supported: bool,
    pub supports_search: bool,
    pub supports_navigation: bool,
    pub supports_inline_render: bool,
}

#[derive(Serialize)]
pub struct ReaderCapabilitiesResponse {
    pub module: String,
    pub version: String,
    pub formats: Vec<ReaderFormatCapabilities>,
    pub legacy_bridge_available: bool,
    pub legacy_bridge_url: String,
}

#[derive(Serialize)]
pub struct ZimArchiveMetaResponse {
    pub filename: String,
    pub size_bytes: u64,
    pub content_url: String,
    pub source_path: String,
}

#[derive(Serialize)]
pub struct ZimReaderCapabilitiesResponse {
    pub filename: String,
    pub mode: String,
    pub supports_native_render: bool,
    pub supports_search: bool,
    pub legacy_bridge_available: bool,
    pub legacy_bridge_url: String,
    pub archive_url: String,
}

#[derive(Deserialize)]
pub struct ZimNativeArticleQuery {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct ZimNativeSearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct ZimNativeSearchItem {
    pub path: String,
    pub title: String,
}

#[derive(Serialize)]
pub struct ZimNativeSearchResponse {
    pub filename: String,
    pub query: String,
    pub results: Vec<ZimNativeSearchItem>,
}

#[derive(Serialize)]
pub struct ZimNativeArticleResponse {
    pub filename: String,
    pub path: String,
    pub title: String,
    pub mime_type: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ReaderOpenResponse {
    pub filename: String,
    pub format: String,
    pub content_url: String,
    pub meta_url: Option<String>,
    pub supports_search: bool,
    pub supports_navigation: bool,
    pub supports_inline_render: bool,
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

    let data_dir_info = get_dir_size(state.config.data_dir.clone());

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

    let (free_bytes, capacity_bytes) = match get_disk_space(&state.config.data_dir) {
        Ok((free, total)) => (Some(free), Some(total)),
        Err(error) => {
            warn!(
                "Failed to read disk space for data directory {}: {}",
                state.config.data_dir.display(),
                error
            );
            (None, None)
        }
    };

    let total_bytes = data_dir_info.0;

    Json(StorageResponse {
        data_dir: state.config.data_dir.display().to_string(),
        total_bytes,
        total_human: format_bytes(total_bytes),
        free_bytes,
        free_human: free_bytes.map(format_bytes),
        capacity_bytes,
        capacity_human: capacity_bytes.map(format_bytes),
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
pub async fn ai_list_models(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<crate::ai::types::ModelRegistryEntry>>, StatusCode> {
    let models = state.model_manager.list_models().await.map_err(|error| {
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
    while let Some(mut field) = multipart.next_field().await.map_err(|error| {
        warn!("Invalid multipart payload while uploading model: {}", error);
        StatusCode::BAD_REQUEST
    })? {
        if field.name() != Some("file") {
            continue;
        }

        let raw_name = field.file_name().ok_or(StatusCode::BAD_REQUEST)?;
        let filename = sanitize_upload_filename(raw_name).ok_or(StatusCode::BAD_REQUEST)?;

        if !filename.to_ascii_lowercase().ends_with(".gguf") {
            return Err(StatusCode::BAD_REQUEST);
        }

        let target_path = state.config.inbox_dir().join(&filename);
        if tokio::fs::try_exists(&target_path).await.map_err(|error| {
            error!(
                "Failed to check existing upload target {}: {}",
                target_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
            tokio::fs::remove_file(&target_path)
                .await
                .map_err(|error| {
                    error!(
                        "Failed to remove existing upload target {}: {}",
                        target_path.display(),
                        error
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }

        let mut file = tokio::fs::File::create(&target_path)
            .await
            .map_err(|error| {
                error!(
                    "Failed to create upload target {}: {}",
                    target_path.display(),
                    error
                );
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

            file.write_all(&chunk).await.map_err(|error| {
                error!(
                    "Failed to write upload target {}: {}",
                    target_path.display(),
                    error
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            size_bytes += chunk.len() as u64;
        }

        if magic.len() < 4 {
            let _ = tokio::fs::remove_file(&target_path).await;
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }

        file.flush().await.map_err(|error| {
            error!(
                "Failed to flush upload target {}: {}",
                target_path.display(),
                error
            );
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

/// POST /api/import/upload — Upload a supported content file into inbox before import
pub async fn upload_file_to_import(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadFileResponse>), StatusCode> {
    while let Some(mut field) = multipart.next_field().await.map_err(|error| {
        warn!(
            "Invalid multipart payload while uploading import file: {}",
            error
        );
        StatusCode::BAD_REQUEST
    })? {
        if field.name() != Some("file") {
            continue;
        }

        let raw_name = field.file_name().ok_or(StatusCode::BAD_REQUEST)?;
        let filename = sanitize_upload_filename(raw_name).ok_or(StatusCode::BAD_REQUEST)?;
        let detected_type = detect_content_type(&filename).ok_or(StatusCode::BAD_REQUEST)?;

        let target_path = state.config.inbox_dir().join(&filename);
        if tokio::fs::try_exists(&target_path).await.map_err(|error| {
            error!(
                "Failed to check existing upload target {}: {}",
                target_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
            tokio::fs::remove_file(&target_path)
                .await
                .map_err(|error| {
                    error!(
                        "Failed to remove existing upload target {}: {}",
                        target_path.display(),
                        error
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }

        let mut file = tokio::fs::File::create(&target_path)
            .await
            .map_err(|error| {
                error!(
                    "Failed to create upload target {}: {}",
                    target_path.display(),
                    error
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let mut size_bytes = 0u64;
        let mut magic = Vec::with_capacity(8);

        while let Some(chunk) = field.chunk().await.map_err(|error| {
            warn!("Failed to read upload stream chunk: {}", error);
            StatusCode::BAD_REQUEST
        })? {
            let needed = 8usize.saturating_sub(magic.len());
            if needed > 0 {
                magic.extend_from_slice(&chunk[..chunk.len().min(needed)]);
            }

            file.write_all(&chunk).await.map_err(|error| {
                error!(
                    "Failed to write upload target {}: {}",
                    target_path.display(),
                    error
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            size_bytes += chunk.len() as u64;
        }

        if size_bytes == 0 {
            let _ = tokio::fs::remove_file(&target_path).await;
            return Err(StatusCode::BAD_REQUEST);
        }

        if !validate_upload_magic(&filename, detected_type, &magic) {
            let _ = tokio::fs::remove_file(&target_path).await;
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }

        file.flush().await.map_err(|error| {
            error!(
                "Failed to flush upload target {}: {}",
                target_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        return Ok((
            StatusCode::CREATED,
            Json(UploadFileResponse {
                filename,
                stored_in: target_path.display().to_string(),
                size_bytes,
                detected_type: Some(detected_type),
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
    let temperature = query.temperature.unwrap_or(DEFAULT_ASSISTANT_TEMPERATURE);
    let max_tokens = query
        .max_tokens
        .unwrap_or(DEFAULT_ASSISTANT_MAX_TOKENS)
        .clamp(1, 4096);
    let num_ctx = query
        .num_ctx
        .unwrap_or_else(|| resolve_assistant_num_ctx(&state.settings_manager.current()))
        .clamp(256, 32768);

    let rx = state
        .model_manager
        .infer_stream(&filename, query.prompt, temperature, max_tokens, num_ctx)
        .await
        .map_err(|error| map_model_error_to_status(&error))?;

    let token_stream = ReceiverStream::new(rx)
        .map(|token| Ok::<Event, Infallible>(Event::default().event("token").data(token)));
    let done_stream = tokio_stream::iter(vec![Ok::<Event, Infallible>(
        Event::default().event("done"),
    )]);
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

/// POST /api/import/download/:filename — Create local import task from inbox
pub async fn create_import_download(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<(StatusCode, Json<CreateDownloadResponse>), StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    let source_path = state.config.inbox_dir().join(&sanitized);

    let exists = tokio::fs::try_exists(&source_path).await.map_err(|error| {
        error!(
            "Failed to verify import source {}: {}",
            source_path.display(),
            error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let task_id = state
        .download_manager
        .create_task(DownloadSource::LocalFile { path: source_path })
        .await;

    Ok((
        StatusCode::CREATED,
        Json(CreateDownloadResponse { task_id }),
    ))
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

/// DELETE /api/download/:task_id/dismiss — Dismiss (remove) a download task
pub async fn dismiss_download(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let dismissed = state
        .download_manager
        .dismiss_task(&task_id)
        .await
        .map_err(|error| {
            error!("Failed to dismiss download task {}: {}", task_id, error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if dismissed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// DELETE /api/content/:content_type/:filename — Permanently delete a content file from disk
pub async fn delete_content_file(
    State(state): State<Arc<AppState>>,
    Path((content_type, filename)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;

    // Defense-in-depth: reject any filename that still contains a path separator after
    // sanitization (sanitize_upload_filename uses Path::file_name which already strips
    // directory components, but this guard is explicit).
    if sanitized.contains('/') || sanitized.contains('\\') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let dir = match content_type.as_str() {
        "maps" => state.config.maps_dir(),
        "books" => state.config.books_dir(),
        "poi" => state.config.poi_dir(),
        "models" => state.config.models_dir(),
        "misc" => state.config.misc_dir(),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let file_path = dir.join(&sanitized);

    // Security: ensure the resolved path stays within the content directory
    let canonical_dir = std::fs::canonicalize(&dir).map_err(|error| {
        error!(
            "Failed to resolve content directory {}: {}",
            dir.display(),
            error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let canonical_path = std::fs::canonicalize(&file_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            error!(
                "Failed to resolve content file path {}: {}",
                file_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(StatusCode::BAD_REQUEST);
    }

    tokio::fs::remove_file(&file_path).await.map_err(|error| {
        error!(
            "Failed to delete content file {}: {}",
            file_path.display(),
            error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/content/:content_type/:filename/download — Download a content file from disk
pub async fn download_content_file(
    State(state): State<Arc<AppState>>,
    Path((content_type, filename)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;

    if sanitized.contains('/') || sanitized.contains('\\') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let dir = match content_type.as_str() {
        "maps" => state.config.maps_dir(),
        "books" => state.config.books_dir(),
        "poi" => state.config.poi_dir(),
        "models" => state.config.models_dir(),
        "misc" => state.config.misc_dir(),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let file_path = dir.join(&sanitized);

    let canonical_dir = std::fs::canonicalize(&dir).map_err(|error| {
        error!(
            "Failed to resolve content directory {}: {}",
            dir.display(),
            error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let canonical_path = std::fs::canonicalize(&file_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            error!(
                "Failed to resolve content file path {}: {}",
                file_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let bytes = tokio::fs::read(&canonical_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            error!(
                "Failed to read content file {}: {}",
                canonical_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment"),
    );

    Ok(response)
}

/// GET /api/reader/capabilities — Unified reader capabilities
pub async fn reader_capabilities() -> Json<ReaderCapabilitiesResponse> {
    Json(ReaderCapabilitiesResponse {
        module: "fyr-unified-reader".to_string(),
        version: "0.1".to_string(),
        formats: vec![
            ReaderFormatCapabilities {
                format: "zim".to_string(),
                supported: true,
                supports_search: true,
                supports_navigation: true,
                supports_inline_render: true,
            },
            ReaderFormatCapabilities {
                format: "epub".to_string(),
                supported: true,
                supports_search: false,
                supports_navigation: true,
                supports_inline_render: true,
            },
            ReaderFormatCapabilities {
                format: "md".to_string(),
                supported: true,
                supports_search: false,
                supports_navigation: true,
                supports_inline_render: true,
            },
        ],
        legacy_bridge_available: false,
        legacy_bridge_url: String::new(),
    })
}

/// GET /api/reader/open/:filename — Resolve unified reader descriptor for a book file
pub async fn reader_open(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Json<ReaderOpenResponse>, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    let format = reader_format_from_filename(&sanitized).ok_or(StatusCode::BAD_REQUEST)?;

    let source_path = state.config.books_dir().join(&sanitized);
    let exists = tokio::fs::try_exists(&source_path).await.map_err(|error| {
        error!(
            "Failed to check reader source {}: {}",
            source_path.display(),
            error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let (supports_search, supports_navigation, supports_inline_render) = match format {
        "zim" => (true, true, false),
        "epub" => (false, true, true),
        "md" => (false, true, true),
        "pdf" => (false, true, true),
        _ => (false, false, false),
    };

    let encoded = sanitized.clone();
    let content_url = format!("/docs/books/{}", encoded);
    let meta_url = if format == "zim" {
        Some(format!("/api/reader/zim/{}/meta", encoded))
    } else {
        None
    };

    Ok(Json(ReaderOpenResponse {
        filename: sanitized,
        format: format.to_string(),
        content_url,
        meta_url,
        supports_search,
        supports_navigation,
        supports_inline_render,
    }))
}

/// GET /api/reader/zim/:filename/meta — ZIM archive metadata for custom reader bootstrap
pub async fn reader_zim_meta(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Json<ZimArchiveMetaResponse>, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    if !sanitized.to_lowercase().ends_with(".zim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let source_path = state.config.books_dir().join(&sanitized);
    let metadata = tokio::fs::metadata(&source_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StatusCode::NOT_FOUND
        } else {
            error!(
                "Failed to read archive metadata {}: {}",
                source_path.display(),
                error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    Ok(Json(ZimArchiveMetaResponse {
        filename: sanitized.clone(),
        size_bytes: metadata.len(),
        content_url: format!("/docs/books/{}", sanitized),
        source_path: source_path.display().to_string(),
    }))
}

/// GET /api/reader/zim/:filename/capabilities — ZIM adapter mode and feature availability
pub async fn reader_zim_capabilities(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Json<ZimReaderCapabilitiesResponse>, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    if !sanitized.to_lowercase().ends_with(".zim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let source_path = state.config.books_dir().join(&sanitized);
    let exists = tokio::fs::try_exists(&source_path).await.map_err(|error| {
        error!(
            "Failed to check ZIM archive existence {}: {}",
            source_path.display(),
            error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }

    let (supports_native_render, mode) = match probe_zim_archive(&source_path) {
        Ok(()) => (true, "native"),
        Err(reason) => {
            warn!(
                "Native ZIM unavailable for {} because archive open failed: {}",
                source_path.display(),
                reason
            );
            (false, "native-unavailable")
        }
    };

    Ok(Json(ZimReaderCapabilitiesResponse {
        filename: sanitized.clone(),
        mode: mode.to_string(),
        supports_native_render,
        supports_search: supports_native_render,
        legacy_bridge_available: false,
        legacy_bridge_url: String::new(),
        archive_url: format!("/docs/books/{}", sanitized),
    }))
}

/// GET /api/reader/zim/:filename/native/article?path=... — Resolve and render a native ZIM article payload
pub async fn reader_zim_native_article(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Query(query): Query<ZimNativeArticleQuery>,
) -> Result<Json<ZimNativeArticleResponse>, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    if !sanitized.to_lowercase().ends_with(".zim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let source_path = state.config.books_dir().join(&sanitized);
    let zim = open_zim_archive(&source_path)?;

    let requested_path = query
        .path
        .as_deref()
        .map(normalize_zim_url)
        .filter(|value| !value.is_empty());

    let entry = if let Some(path) = requested_path {
        find_entry_by_path_flexible(&zim, &path).ok_or(StatusCode::NOT_FOUND)?
    } else {
        resolve_main_entry(&zim).ok_or(StatusCode::NOT_FOUND)?
    };

    let (path, title, mime_type, bytes) = resolve_entry_bytes(&zim, &entry)?;
    let content = String::from_utf8_lossy(&bytes).into_owned();

    Ok(Json(ZimNativeArticleResponse {
        filename: sanitized,
        path,
        title,
        mime_type,
        content,
    }))
}

/// GET /api/reader/zim/:filename/native/content/*path — Serve a native resource blob by ZIM path
pub async fn reader_zim_native_content(
    State(state): State<Arc<AppState>>,
    Path((filename, path)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    if !sanitized.to_lowercase().ends_with(".zim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let normalized_path = normalize_zim_url(&path);
    if normalized_path.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let source_path = state.config.books_dir().join(&sanitized);
    let zim = open_zim_archive(&source_path)?;
    let mut entry = content_path_lookup_variants(&normalized_path)
        .into_iter()
        .find_map(|candidate| find_any_entry_by_path(&zim, &candidate));

    if entry.is_none() && normalized_path.starts_with("_assets_/") {
        if let Some(filename_only) = normalized_path.rsplit('/').next() {
            entry = find_any_entry_by_basename(&zim, filename_only);
        }
    }

    let entry = entry.ok_or(StatusCode::NOT_FOUND)?;
    let (_, _, mime_type, bytes) = resolve_entry_bytes(&zim, &entry)?;

    let header_value = HeaderValue::from_str(&mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, header_value);
    Ok(response)
}

/// GET /api/reader/zim/:filename/native/search?q=...&limit=... — Search article titles/paths
pub async fn reader_zim_native_search(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Query(query): Query<ZimNativeSearchQuery>,
) -> Result<Json<ZimNativeSearchResponse>, StatusCode> {
    let sanitized = sanitize_upload_filename(&filename).ok_or(StatusCode::BAD_REQUEST)?;
    if !sanitized.to_lowercase().ends_with(".zim") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let needle = query.q.trim();
    if needle.is_empty() {
        return Ok(Json(ZimNativeSearchResponse {
            filename: sanitized,
            query: String::new(),
            results: Vec::new(),
        }));
    }

    let source_path = state.config.books_dir().join(&sanitized);
    let zim = open_zim_archive(&source_path)?;
    let needle_lower = needle.to_ascii_lowercase();
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let mut results: Vec<ZimNativeSearchItem> = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    for entry_result in zim.iterate_by_urls() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                warn!(
                    "Skipping invalid ZIM entry while searching {}: {}",
                    sanitized, error
                );
                continue;
            }
        };

        if !is_searchable_article_entry(&entry) {
            continue;
        }

        let path = normalize_zim_url(&entry.url);
        if path.is_empty() || path.starts_with("_assets_/") {
            continue;
        }

        let title = if entry.title.is_empty() {
            entry.url.replace('_', " ")
        } else {
            entry.title.clone()
        };

        let title_norm = title.to_ascii_lowercase();
        let path_norm = path.to_ascii_lowercase();
        if !title_norm.contains(&needle_lower) && !path_norm.contains(&needle_lower) {
            continue;
        }

        let resolved = match resolve_redirect(&zim, entry, 0) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let resolved_path = normalize_zim_url(&resolved.url);
        if resolved_path.is_empty() || resolved_path.starts_with("_assets_/") {
            continue;
        }

        if !seen_paths.insert(resolved_path.clone()) {
            continue;
        }

        let resolved_title = if resolved.title.is_empty() {
            resolved_path.replace('_', " ")
        } else {
            resolved.title.clone()
        };

        results.push(ZimNativeSearchItem {
            path: resolved_path,
            title: resolved_title,
        });
        if results.len() >= limit {
            break;
        }
    }

    Ok(Json(ZimNativeSearchResponse {
        filename: sanitized,
        query: needle.to_string(),
        results,
    }))
}

/// GET / — Fallback handler for SPA (serves index.html)
/// This enables client-side routing in Vue
pub async fn serve_index(State(state): State<Arc<AppState>>) -> Response {
    let index_path = state.static_dir.join("index.html");

    match tokio::fs::read_to_string(&index_path).await {
        Ok(content) => Html(content).into_response(),
        Err(err) => {
            error!(
                "Failed to read SPA index at {}: {}",
                index_path.display(),
                err
            );

            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("Fyr static frontend is unavailable: index.html not found.".to_string()),
            )
                .into_response()
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
            let title = if content_type == ContentType::Book {
                extract_book_title(&path)
            } else {
                None
            };
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
                title,
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

/// Try to extract the human-readable title embedded in a book file.
/// Returns `None` if the format is unsupported or the title cannot be read.
fn extract_book_title(path: &FsPath) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "epub" => extract_epub_title(path),
        "zim" => extract_zim_title(path),
        _ => None,
    }
}

/// Extract the `dc:title` from an EPUB's OPF package document.
fn extract_epub_title(path: &FsPath) -> Option<String> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    // Locate the OPF file via META-INF/container.xml
    let container_xml = {
        let mut entry = archive.by_name("META-INF/container.xml").ok()?;
        let mut content = String::new();
        entry.read_to_string(&mut content).ok()?;
        content
    };
    let opf_path = extract_xml_attr(&container_xml, "full-path")?;

    // Read the OPF package document
    let opf_content = {
        let mut entry = archive.by_name(&opf_path).ok()?;
        let mut content = String::new();
        entry.read_to_string(&mut content).ok()?;
        content
    };

    // Extract the title from <dc:title>
    extract_xml_text_content(&opf_content, "dc:title")
}

/// Extract the archive-level title from a ZIM file's `M/Title` metadata entry.
fn extract_zim_title(path: &FsPath) -> Option<String> {
    // The ZIM library can panic on malformed archives; catch_unwind mirrors the
    // approach used by open_zim_archive / probe_zim_archive elsewhere in this module.
    let zim = std::panic::catch_unwind(AssertUnwindSafe(|| Zim::new(path)))
        .ok()? // outer Ok: convert panic result to Option (None on panic)
        .ok()?; // inner Ok: convert Zim::new's Result to Option (None on error)

    let content = zim.metadata("Title").ok()??;
    let blob = content.to_vec().ok()?;
    let title = String::from_utf8_lossy(&blob).trim().to_string();

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Extract the value of `attr="..."` or `attr='...'` (with optional whitespace around `=`)
/// from a snippet of XML.  Handles both single- and double-quoted attribute values.
fn extract_xml_attr(xml: &str, attr: &str) -> Option<String> {
    let attr_start = xml.find(attr)?;
    let after_attr = xml[attr_start + attr.len()..].trim_start();
    let after_eq = after_attr.strip_prefix('=')?;
    let rest = after_eq.trim_start();
    let (quote, inner) = if let Some(s) = rest.strip_prefix('"') {
        ('"', s)
    } else if let Some(s) = rest.strip_prefix('\'') {
        ('\'', s)
    } else {
        return None;
    };
    let end = inner.find(quote)?;
    let value = inner[..end].trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Extract the plain-text content of the first `<tag …>…</tag>` element in an XML
/// snippet.  Attributes on the opening tag are skipped correctly, and the returned
/// value has leading/trailing whitespace trimmed.
///
/// This helper covers well-formed EPUB OPF and ZIM metadata XML.  It does not
/// handle CDATA sections, XML comments, or nested elements of the same tag.
fn extract_xml_text_content(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);
    let tag_start = xml.find(&open_tag)?;
    // Skip past the closing `>` of the opening tag (which may carry attributes).
    let content_start = xml[tag_start..].find('>')? + tag_start + 1;
    let content_end = xml.find(&close_tag)?;
    if content_end <= content_start {
        return None;
    }
    let text = xml[content_start..content_end].trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
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

fn get_disk_space(path: &FsPath) -> std::io::Result<(u64, u64)> {
    let free = fs2::available_space(path)?;
    let total = fs2::total_space(path)?;
    Ok((free, total))
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

fn model_error_response(
    error: crate::ai::error::ModelError,
) -> (StatusCode, Json<ErrorMessageResponse>) {
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

fn detect_content_type(filename: &str) -> Option<ContentType> {
    let extension = FsPath::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    ContentType::from_extension(extension)
}

fn reader_format_from_filename(filename: &str) -> Option<&'static str> {
    let extension = FsPath::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "zim" => Some("zim"),
        "epub" => Some("epub"),
        "md" => Some("md"),
        "pdf" => Some("pdf"),
        _ => None,
    }
}

fn open_zim_archive(path: &std::path::Path) -> Result<Zim, StatusCode> {
    match std::panic::catch_unwind(AssertUnwindSafe(|| Zim::new(path))) {
        Ok(Ok(zim)) => Ok(zim),
        Ok(Err(error)) => {
            error!("Failed to open ZIM archive {}: {}", path.display(), error);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(_) => {
            error!(
                "ZIM parser panicked while opening archive {}",
                path.display()
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

fn probe_zim_archive(path: &std::path::Path) -> Result<(), String> {
    match std::panic::catch_unwind(AssertUnwindSafe(|| Zim::new(path))) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("zim parser panic".to_string()),
    }
}

fn resolve_main_entry(zim: &Zim) -> Option<DirectoryEntry> {
    if let Some(main_page_idx) = zim.header.main_page {
        if let Ok(entry) = zim.get_by_url_index(main_page_idx) {
            return resolve_redirect(zim, entry, 0).ok();
        }
    }

    zim.iterate_by_urls()
        .filter_map(Result::ok)
        .find(|entry| matches!(entry.namespace, Namespace::Articles))
        .and_then(|entry| resolve_redirect(zim, entry, 0).ok())
}

fn find_entry_by_path_flexible(zim: &Zim, path: &str) -> Option<DirectoryEntry> {
    let normalized = normalize_zim_url(path);
    if normalized.is_empty() {
        return None;
    }

    let variants = path_lookup_variants(&normalized);

    let direct = zim.iterate_by_urls().filter_map(Result::ok).find(|entry| {
        let entry_url = normalize_zim_url(&entry.url);
        let entry_title = normalize_zim_url(&entry.title);
        variants
            .iter()
            .any(|candidate| entry_url == *candidate || entry_title == *candidate)
    })?;

    resolve_redirect(zim, direct, 0).ok()
}

fn path_lookup_variants(path: &str) -> Vec<String> {
    let trimmed = normalize_zim_url(path);
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut variants = vec![trimmed.clone()];

    let spaces = trimmed.replace('_', " ");
    if spaces != trimmed {
        variants.push(spaces);
    }

    let underscores = trimmed.replace(' ', "_");
    if underscores != trimmed {
        variants.push(underscores);
    }

    variants.sort();
    variants.dedup();
    variants
}

fn content_path_lookup_variants(path: &str) -> Vec<String> {
    let normalized = normalize_zim_url(path);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut variants = path_lookup_variants(&normalized);

    if normalized.starts_with("_assets_/") {
        let parts: Vec<&str> = normalized.split('/').collect();
        if parts.len() > 2 {
            let without_hash = format!("_assets_/{}", parts[2..].join("/"));
            variants.push(without_hash);

            if let Some(last) = parts.last() {
                variants.push(format!("I/{}", last));
            }
        }
    }

    variants.sort();
    variants.dedup();
    variants
}

fn find_any_entry_by_path(zim: &Zim, path: &str) -> Option<DirectoryEntry> {
    let normalized = normalize_zim_url(path);
    if normalized.is_empty() {
        return None;
    }

    let direct = zim.iterate_by_urls().filter_map(Result::ok).find(|entry| {
        normalize_zim_url(&entry.url) == normalized || normalize_zim_url(&entry.title) == normalized
    })?;

    resolve_redirect(zim, direct, 0).ok()
}

fn find_any_entry_by_basename(zim: &Zim, filename: &str) -> Option<DirectoryEntry> {
    let needle = normalize_zim_url(filename);
    if needle.is_empty() {
        return None;
    }

    let direct = zim.iterate_by_urls().filter_map(Result::ok).find(|entry| {
        let url = normalize_zim_url(&entry.url);
        let title = normalize_zim_url(&entry.title);
        url.rsplit('/').next().is_some_and(|name| name == needle)
            || title.rsplit('/').next().is_some_and(|name| name == needle)
    })?;

    resolve_redirect(zim, direct, 0).ok()
}

fn is_searchable_article_entry(entry: &DirectoryEntry) -> bool {
    if matches!(entry.namespace, Namespace::Articles) {
        return true;
    }

    matches!(
        &entry.mime_type,
        MimeType::Type(value)
            if value.starts_with("text/html") || value.starts_with("application/xhtml+xml")
    )
}

fn resolve_redirect(
    zim: &Zim,
    entry: DirectoryEntry,
    _depth: usize,
) -> zim::Result<DirectoryEntry> {
    zim.resolve(entry)
}

fn resolve_entry_bytes(
    zim: &Zim,
    entry: &DirectoryEntry,
) -> Result<(String, String, String, Vec<u8>), StatusCode> {
    let content = zim
        .entry_content(entry)
        .map_err(|error| {
            error!(
                "Failed to resolve ZIM content handle for {}: {}",
                entry.url, error
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let bytes = content.to_vec().map_err(|error| {
        error!(
            "Failed to read ZIM content bytes for {}: {}",
            entry.url, error
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mime_type = match &entry.mime_type {
        MimeType::Type(value) => value.clone(),
        MimeType::Redirect => "text/plain; charset=utf-8".to_string(),
        MimeType::LinkTarget => "application/octet-stream".to_string(),
        MimeType::DeletedEntry => "application/octet-stream".to_string(),
    };

    let title = if entry.title.is_empty() {
        entry.url.clone()
    } else {
        entry.title.clone()
    };

    Ok((normalize_zim_url(&entry.url), title, mime_type, bytes))
}

fn normalize_zim_url(value: &str) -> String {
    let raw = value
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_start_matches('/');

    let mut out = raw.to_string();
    for _ in 0..3 {
        let decoded = decode_percent_once(&out);
        if decoded == out {
            break;
        }
        out = decoded;
    }

    out
}

fn decode_percent_once(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            if let (Some(hi), Some(lo)) = (hex_nibble(hi), hex_nibble(lo)) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }

        out.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn resolve_assistant_num_ctx(settings: &AppSettings) -> usize {
    assistant_num_ctx_override(settings).unwrap_or_else(default_assistant_num_ctx)
}

fn assistant_num_ctx_override(settings: &AppSettings) -> Option<usize> {
    let assistant = settings.modules.get("assistant")?.as_object()?;

    if let Some(num_ctx) = assistant.get("num_ctx").and_then(|value| value.as_u64()) {
        return usize::try_from(num_ctx).ok().filter(|value| *value > 0);
    }

    if assistant
        .get("high_ram_context")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Some(HIGH_RAM_ASSISTANT_NUM_CTX);
    }

    None
}

fn default_assistant_num_ctx() -> usize {
    match detect_total_memory_kib() {
        Some(total_kib) if total_kib > HIGH_RAM_THRESHOLD_KIB => HIGH_RAM_ASSISTANT_NUM_CTX,
        _ => DEFAULT_ASSISTANT_NUM_CTX,
    }
}

fn detect_total_memory_kib() -> Option<u64> {
    let raw = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_total_memory_kib(&raw)
}

fn parse_total_memory_kib(meminfo: &str) -> Option<u64> {
    meminfo.lines().find_map(|line| {
        let rest = line.strip_prefix("MemTotal:")?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::{
        assistant_num_ctx_override, parse_total_memory_kib, reader_format_from_filename,
        resolve_assistant_num_ctx, sanitize_upload_filename, DEFAULT_ASSISTANT_NUM_CTX,
        HIGH_RAM_ASSISTANT_NUM_CTX,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use types::AppSettings;

    #[test]
    fn detects_supported_reader_formats() {
        assert_eq!(reader_format_from_filename("archive.zim"), Some("zim"));
        assert_eq!(reader_format_from_filename("novel.epub"), Some("epub"));
        assert_eq!(reader_format_from_filename("guide.md"), Some("md"));
        assert_eq!(reader_format_from_filename("paper.pdf"), Some("pdf"));
    }

    #[test]
    fn rejects_unsupported_reader_formats() {
        assert_eq!(reader_format_from_filename("model.gguf"), None);
        assert_eq!(reader_format_from_filename("notes.txt"), None);
        assert_eq!(reader_format_from_filename("no_extension"), None);
    }

    #[test]
    fn sanitizes_filename_to_leaf_component() {
        assert_eq!(
            sanitize_upload_filename("nested/path/atlas.zim"),
            Some("atlas.zim".to_string())
        );
        assert_eq!(sanitize_upload_filename("  "), None);
    }

    #[test]
    fn parses_memtotal_from_proc_meminfo() {
        let meminfo = "MemTotal:       32768000 kB\nMemFree:         1024000 kB\n";
        assert_eq!(parse_total_memory_kib(meminfo), Some(32_768_000));
    }

    #[test]
    fn assistant_num_ctx_override_prefers_explicit_num_ctx() {
        let mut modules = HashMap::new();
        modules.insert(
            "assistant".to_string(),
            json!({
                "num_ctx": 4096,
                "high_ram_context": true
            }),
        );

        let settings = AppSettings {
            location: None,
            modules,
        };

        assert_eq!(assistant_num_ctx_override(&settings), Some(4096));
    }

    #[test]
    fn assistant_num_ctx_override_supports_high_ram_flag() {
        let mut modules = HashMap::new();
        modules.insert(
            "assistant".to_string(),
            json!({
                "high_ram_context": true
            }),
        );

        let settings = AppSettings {
            location: None,
            modules,
        };

        assert_eq!(
            assistant_num_ctx_override(&settings),
            Some(HIGH_RAM_ASSISTANT_NUM_CTX)
        );
    }

    #[test]
    fn resolve_assistant_num_ctx_uses_default_without_override() {
        let settings = AppSettings::default();
        let resolved = resolve_assistant_num_ctx(&settings);
        assert!(resolved == DEFAULT_ASSISTANT_NUM_CTX || resolved == HIGH_RAM_ASSISTANT_NUM_CTX);
    }
}

fn validate_upload_magic(filename: &str, content_type: ContentType, magic: &[u8]) -> bool {
    match content_type {
        ContentType::Model => magic.len() >= 4 && &magic[0..4] == b"GGUF",
        ContentType::Map => magic.len() >= 7 && &magic[0..7] == b"PMTiles",
        ContentType::Book => {
            let ext = FsPath::new(filename)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if ext == "epub" {
                magic.len() >= 4 && magic[0..4] == [0x50, 0x4B, 0x03, 0x04]
            } else {
                true
            }
        }
        ContentType::Poi | ContentType::Misc => true,
    }
}
