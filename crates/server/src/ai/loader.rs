use super::error::ModelError;
use super::types::ModelMetadata;
use candle_core::quantized::gguf_file;
use candle_core::quantized::tokenizer::TokenizerFromGguf;
use candle_transformers::models::quantized_qwen2;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;

#[derive(Clone)]
pub struct LoadedModel {
    pub metadata: ModelMetadata,
    pub runtime: ModelRuntime,
}

#[derive(Clone)]
pub enum ModelRuntime {
    ValidationOnly { reason: Option<String> },
    QuantizedQwen2 {
        model: Arc<Mutex<quantized_qwen2::ModelWeights>>,
        tokenizer: Arc<Tokenizer>,
        eos_token_ids: Vec<u32>,
    },
}

impl LoadedModel {
    pub fn supports_inference(&self) -> bool {
        matches!(self.runtime, ModelRuntime::QuantizedQwen2 { .. })
    }

    pub fn validation_reason(&self) -> Option<String> {
        match &self.runtime {
            ModelRuntime::ValidationOnly { reason } => reason.clone(),
            ModelRuntime::QuantizedQwen2 { .. } => None,
        }
    }
}

pub struct ModelLoader;

impl ModelLoader {
    pub fn load(path: &Path) -> Result<LoadedModel, ModelError> {
        if !path.exists() {
            return Err(ModelError::NotFound(path.display().to_string()));
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
        if !ext.eq_ignore_ascii_case("gguf") {
            return Err(ModelError::InvalidExtension(path.display().to_string()));
        }

        let mut magic_reader = File::open(path)
            .map_err(|e| ModelError::ImportFailed(format!("{} ({})", path.display(), e)))?;
        let mut magic = [0u8; 4];
        magic_reader
            .read_exact(&mut magic)
            .map_err(|e| ModelError::InvalidMagic(format!("{} ({})", path.display(), e)))?;
        if magic != *b"GGUF" {
            return Err(ModelError::InvalidMagic(path.display().to_string()));
        }

        let mut file = File::open(path)
            .map_err(|e| ModelError::ImportFailed(format!("{} ({})", path.display(), e)))?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| ModelError::GgufParse(e.to_string()))?;

        let architecture = content
            .metadata
            .get("general.architecture")
            .and_then(|v| v.to_string().ok())
            .map(|s| s.to_string());

        let has_tokenizer_metadata = content
            .metadata
            .keys()
            .any(|key| key.starts_with("tokenizer."));

        if !has_tokenizer_metadata {
            return Err(ModelError::MissingTokenizerMetadata(path.display().to_string()));
        }

        let size_bytes = std::fs::metadata(path)
            .map_err(|e| ModelError::ImportFailed(e.to_string()))?
            .len();
        let tensor_count = content.tensor_infos.len();

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.gguf")
            .to_string();

        let runtime = match architecture.as_deref() {
            Some("qwen2") => load_quantized_qwen2_runtime(path, content, &filename)?,
            _ => ModelRuntime::ValidationOnly {
                reason: Some("Inference is not implemented for this model architecture in Fyr yet.".to_string()),
            },
        };

        let metadata = ModelMetadata {
            filename,
            size_bytes,
            architecture,
            tensor_count,
            has_tokenizer_metadata,
        };

        Ok(LoadedModel {
            metadata,
            runtime,
        })
    }
}

fn load_quantized_qwen2_runtime(
    path: &Path,
    content: gguf_file::Content,
    filename: &str,
) -> Result<ModelRuntime, ModelError> {
    let tokenizer = Tokenizer::from_gguf(&content)
        .map_err(|e| ModelError::InferenceFailed(format!(
            "failed to build tokenizer from GGUF metadata for {filename}: {e}"
        )))?;

    let device = candle_core::Device::Cpu;
    let mut file = File::open(path)
        .map_err(|e| ModelError::ImportFailed(format!("{} ({})", path.display(), e)))?;
    let model = quantized_qwen2::ModelWeights::from_gguf(content, &mut file, &device)
        .map_err(|e| ModelError::GgufParse(e.to_string()))?;

    Ok(ModelRuntime::QuantizedQwen2 {
        model: Arc::new(Mutex::new(model)),
        tokenizer: Arc::new(tokenizer.clone()),
        eos_token_ids: eos_token_ids(&tokenizer),
    })
}

fn eos_token_ids(tokenizer: &Tokenizer) -> Vec<u32> {
    ["<|im_end|>", "<|endoftext|>", "</s>", "<|eot_id|>"]
        .into_iter()
        .filter_map(|token| tokenizer.token_to_id(token))
        .collect()
}
