//! API request/response handlers

use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    Json,
};
use types::{ContentMetadata, ContentType, DownloadSource};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use walkdir::WalkDir;

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
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub data_dir: String,
    pub server_host: String,
    pub server_port: u16,
    pub directories: DirectoriesResponse,
}

#[derive(Serialize)]
pub struct DirectoriesResponse {
    pub maps: String,
    pub books: String,
    pub poi: String,
    pub inbox: String,
}

#[derive(Deserialize)]
pub struct CreateDownloadRequest {
    pub url: String,
}

#[derive(Serialize)]
pub struct CreateDownloadResponse {
    pub task_id: String,
}

/// GET /api/status — Server status and content inventory
pub async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let maps_count = count_files(state.config.maps_dir());
    let books_count = count_files(state.config.books_dir());
    let poi_count = count_files(state.config.poi_dir());

    Json(StatusResponse {
        version: "0.1.0".to_string(),
        status: "running".to_string(),
        data_dir: state.config.data_dir.display().to_string(),
        content_count: ContentCountResponse {
            maps: maps_count,
            books: books_count,
            poi: poi_count,
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
        },
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

/// Fallback handler — serve UI (placeholder for now)
pub async fn serve_ui() -> Html<&'static str> {
    Html(
        r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Offline Nexus</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f5f5f5; }
        .container { max-width: 1200px; margin: 0 auto; padding: 20px; }
        .header { background: #333; color: white; padding: 20px; border-radius: 8px; margin-bottom: 20px; }
        h1 { margin-bottom: 10px; }
        .status { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 15px; margin-bottom: 30px; }
        .card { background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        .card h2 { font-size: 14px; color: #666; margin-bottom: 10px; text-transform: uppercase; }
        .card .value { font-size: 32px; font-weight: bold; color: #333; }
        .loading { text-align: center; padding: 40px; color: #999; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>📦 Offline Nexus</h1>
            <p>Off-grid content platform — maps, books, POIs</p>
        </div>
        
        <div class="status" id="status">
            <div class="loading">Loading status...</div>
        </div>
        
        <div class="card">
            <h2>Quick Links</h2>
            <ul style="list-style: none;">
                <li><a href="/api/status">API Status</a></li>
                <li><a href="/api/config">Configuration</a></li>
                <li><a href="/api/content/maps">Maps</a></li>
                <li><a href="/api/content/books">Books</a></li>
                <li><a href="/api/content/poi">POIs</a></li>
            </ul>
        </div>
    </div>
    
    <script>
        fetch('/api/status')
            .then(r => r.json())
            .then(data => {
                const html = `
                    <div class="card">
                        <h2>Maps</h2>
                        <div class="value">${data.content_count.maps}</div>
                    </div>
                    <div class="card">
                        <h2>Books</h2>
                        <div class="value">${data.content_count.books}</div>
                    </div>
                    <div class="card">
                        <h2>POI Collections</h2>
                        <div class="value">${data.content_count.poi}</div>
                    </div>
                `;
                document.getElementById('status').innerHTML = html;
            })
            .catch(e => console.error('Error loading status:', e));
    </script>
</body>
</html>
"#,
    )
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
