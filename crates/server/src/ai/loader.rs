use super::error::ModelError;
use super::types::ModelMetadata;
use candle_core::quantized::gguf_file;
use candle_transformers::quantized_var_builder::VarBuilder;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone)]
pub struct LoadedModel {
    pub metadata: ModelMetadata,
    pub _weights: VarBuilder,
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

        let device = candle_core::Device::Cpu;
        let weights = VarBuilder::from_gguf(path, &device)
            .map_err(|e| ModelError::GgufParse(e.to_string()))?;

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.gguf")
            .to_string();

        let metadata = ModelMetadata {
            filename,
            size_bytes,
            architecture,
            tensor_count: content.tensor_infos.len(),
            has_tokenizer_metadata,
        };

        Ok(LoadedModel {
            metadata,
            _weights: weights,
        })
    }
}
