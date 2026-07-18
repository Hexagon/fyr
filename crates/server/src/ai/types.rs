use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ModelMetadata {
    pub filename: String,
    pub size_bytes: u64,
    pub architecture: Option<String>,
    pub tensor_count: usize,
    pub has_tokenizer_metadata: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelLoadState {
    Unloaded,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRegistryEntry {
    pub filename: String,
    pub size_bytes: u64,
    pub loaded: bool,
    pub state: ModelLoadState,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportModelRequest {
    pub filename: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct ImportModelResponse {
    pub filename: String,
    pub imported_to: String,
}

#[derive(Debug, Serialize)]
pub struct UploadModelResponse {
    pub filename: String,
    pub stored_in: String,
    pub size_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct InferStreamQuery {
    pub prompt: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LoadModelResponse {
    pub filename: String,
    pub state: ModelLoadState,
}

#[derive(Debug, Serialize)]
pub struct ModelHealthResponse {
    pub filename: String,
    pub loaded: bool,
    pub state: ModelLoadState,
    pub architecture: Option<String>,
    pub has_tokenizer_metadata: bool,
    pub error: Option<String>,
}
