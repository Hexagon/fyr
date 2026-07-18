use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("model file not found: {0}")]
    NotFound(String),

    #[error("invalid model file extension, expected .gguf: {0}")]
    InvalidExtension(String),

    #[error("model file is not a valid GGUF archive (missing GGUF magic bytes): {0}")]
    InvalidMagic(String),

    #[error("failed to parse GGUF content: {0}")]
    GgufParse(String),

    #[error("GGUF file is missing tokenizer metadata: {0}")]
    MissingTokenizerMetadata(String),

    #[error("model is not loaded: {0}")]
    NotLoaded(String),

    #[error("failed to import model file: {0}")]
    ImportFailed(String),

    #[error("failed to run inference: {0}")]
    InferenceFailed(String),

    #[error("internal model error: {0}")]
    Internal(String),
}
