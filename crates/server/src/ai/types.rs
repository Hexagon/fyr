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
    pub num_ctx: Option<usize>,
    /// JSON-encoded array of `{"role":"user"|"assistant","text":"..."}` objects
    /// representing prior conversation turns (most recent last).
    pub history: Option<String>,
    /// Ground the answer in the offline library ("ask your library"):
    /// retrieve passages, inject them as numbered sources and emit a
    /// `sources` SSE event before the first token.
    pub use_library: Option<bool>,
    /// Maximum number of library sources (default 4, clamped to 1..=8).
    pub sources_limit: Option<usize>,
}

/// JSON body for `POST /api/models/:filename/infer/stream`. Same fields as
/// [`InferStreamQuery`], but `history` is a real array, so long conversations
/// are not limited by URL length.
#[derive(Debug, Deserialize)]
pub struct InferStreamBody {
    pub prompt: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<usize>,
    pub num_ctx: Option<usize>,
    #[serde(default)]
    pub history: Vec<ChatMessage>,
    pub use_library: Option<bool>,
    pub sources_limit: Option<usize>,
}

impl From<InferStreamQuery> for InferStreamBody {
    fn from(query: InferStreamQuery) -> Self {
        Self {
            prompt: query.prompt,
            temperature: query.temperature,
            max_tokens: query.max_tokens,
            num_ctx: query.num_ctx,
            history: query
                .history
                .as_deref()
                .and_then(|h| serde_json::from_str::<Vec<ChatMessage>>(h).ok())
                .unwrap_or_default(),
            use_library: query.use_library,
            sources_limit: query.sources_limit,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
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
