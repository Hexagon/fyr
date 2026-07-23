use super::error::ModelError;
use super::types::ModelMetadata;
use candle_core::quantized::gguf_file;
use candle_core::quantized::gguf_file::Value;
use candle_core::quantized::tokenizer::TokenizerFromGguf;
use candle_transformers::models::quantized_llama;
use candle_transformers::models::quantized_phi;
use candle_transformers::models::quantized_phi3;
use candle_transformers::models::quantized_qwen2;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizers::tokenizer::SplitDelimiterBehavior;
use tokenizers::{
    decoders::metaspace::{Metaspace as MetaspaceDecoder, PrependScheme},
    decoders::{byte_level::ByteLevel as ByteLevelDecoder, DecoderWrapper},
    models::bpe::{BPE, Vocab},
    models::unigram::Unigram,
    normalizers::{unicode::NFC, NormalizerWrapper},
    pre_tokenizers::{
        byte_level::ByteLevel as ByteLevelPre,
        metaspace::Metaspace,
        sequence::Sequence,
        split::{Split, SplitPattern},
        PreTokenizerWrapper,
    },
    processors::sequence::Sequence as ProcessorSequence,
    processors::{byte_level::ByteLevel as ByteLevelProcessor, PostProcessorWrapper},
    AddedToken, Tokenizer,
};

const PHI3_MAX_CONTEXT_LENGTH: u32 = 8192;

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
    QuantizedLlama {
        model: Arc<Mutex<quantized_llama::ModelWeights>>,
        tokenizer: Arc<Tokenizer>,
        eos_token_ids: Vec<u32>,
    },
    QuantizedPhi {
        model: Arc<Mutex<quantized_phi::ModelWeights>>,
        tokenizer: Arc<Tokenizer>,
        eos_token_ids: Vec<u32>,
    },
    QuantizedPhi3 {
        model: Arc<Mutex<quantized_phi3::ModelWeights>>,
        tokenizer: Arc<Tokenizer>,
        eos_token_ids: Vec<u32>,
    },
}

impl LoadedModel {
    pub fn supports_inference(&self) -> bool {
        matches!(
            self.runtime,
            ModelRuntime::QuantizedQwen2 { .. }
                | ModelRuntime::QuantizedLlama { .. }
                | ModelRuntime::QuantizedPhi { .. }
                | ModelRuntime::QuantizedPhi3 { .. }
        )
    }

    pub fn validation_reason(&self) -> Option<String> {
        match &self.runtime {
            ModelRuntime::ValidationOnly { reason } => reason.clone(),
            ModelRuntime::QuantizedQwen2 { .. }
            | ModelRuntime::QuantizedLlama { .. }
            | ModelRuntime::QuantizedPhi { .. }
            | ModelRuntime::QuantizedPhi3 { .. } => None,
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
            Some("llama") => load_quantized_llama_runtime(path, content, &filename)?,
            Some("phi") => load_quantized_phi_runtime(path, content, &filename)?,
            Some("phi3") => load_quantized_phi3_runtime(path, content, &filename)?,
            _ => ModelRuntime::ValidationOnly {
                reason: Some("Inference is not implemented for this model architecture in Fyr yet. Supported architectures are qwen2, llama, phi, and phi3.".to_string()),
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
    let tokenizer = load_tokenizer(path, &content, filename)?;
    let device = candle_core::Device::Cpu;
    let mut file = open_runtime_file(path)?;
    let model = quantized_qwen2::ModelWeights::from_gguf(content, &mut file, &device)
        .map_err(|e| ModelError::GgufParse(e.to_string()))?;

    Ok(ModelRuntime::QuantizedQwen2 {
        model: Arc::new(Mutex::new(model)),
        tokenizer: Arc::new(tokenizer.clone()),
        eos_token_ids: eos_token_ids(&tokenizer),
    })
}

fn load_quantized_llama_runtime(
    path: &Path,
    content: gguf_file::Content,
    filename: &str,
) -> Result<ModelRuntime, ModelError> {
    let tokenizer = load_tokenizer(path, &content, filename)?;
    let device = candle_core::Device::Cpu;
    let mut file = open_runtime_file(path)?;
    let model = quantized_llama::ModelWeights::from_gguf(content, &mut file, &device)
        .map_err(|e| ModelError::GgufParse(e.to_string()))?;

    Ok(ModelRuntime::QuantizedLlama {
        model: Arc::new(Mutex::new(model)),
        tokenizer: Arc::new(tokenizer.clone()),
        eos_token_ids: eos_token_ids(&tokenizer),
    })
}

fn load_quantized_phi_runtime(
    path: &Path,
    content: gguf_file::Content,
    filename: &str,
) -> Result<ModelRuntime, ModelError> {
    let tokenizer = load_tokenizer(path, &content, filename)?;
    let device = candle_core::Device::Cpu;
    let mut file = open_runtime_file(path)?;
    let model = quantized_phi::ModelWeights::from_gguf(content, &mut file, &device)
        .map_err(|e| ModelError::GgufParse(e.to_string()))?;

    Ok(ModelRuntime::QuantizedPhi {
        model: Arc::new(Mutex::new(model)),
        tokenizer: Arc::new(tokenizer.clone()),
        eos_token_ids: eos_token_ids(&tokenizer),
    })
}

fn load_quantized_phi3_runtime(
    path: &Path,
    mut content: gguf_file::Content,
    filename: &str,
) -> Result<ModelRuntime, ModelError> {
    cap_phi3_context_length(&mut content);
    let tokenizer = load_tokenizer(path, &content, filename)?;
    let device = candle_core::Device::Cpu;
    let mut file = open_runtime_file(path)?;
    let model = quantized_phi3::ModelWeights::from_gguf(false, content, &mut file, &device)
        .map_err(|e| ModelError::GgufParse(e.to_string()))?;

    Ok(ModelRuntime::QuantizedPhi3 {
        model: Arc::new(Mutex::new(model)),
        tokenizer: Arc::new(tokenizer.clone()),
        eos_token_ids: eos_token_ids(&tokenizer),
    })
}

fn cap_phi3_context_length(content: &mut gguf_file::Content) {
    let key = "phi3.context_length";
    let capped = content.metadata.get(key).and_then(|value| match value {
        Value::U8(v) => Some((*v as u32).min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::I8(v) => Some((*v as i64).max(0) as u32).map(|v| v.min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::U16(v) => Some((*v as u32).min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::I16(v) => Some((*v as i64).max(0) as u32).map(|v| v.min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::U32(v) => Some((*v).min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::I32(v) => Some((*v as i64).max(0) as u32).map(|v| v.min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::U64(v) => Some((*v as u32).min(PHI3_MAX_CONTEXT_LENGTH)),
        Value::I64(v) => Some((*v).max(0) as u32).map(|v| v.min(PHI3_MAX_CONTEXT_LENGTH)),
        _ => None,
    });

    if let Some(max_len) = capped {
        content
            .metadata
            .insert(key.to_string(), Value::U32(max_len));
    }
}

fn load_tokenizer(
    model_path: &Path,
    content: &gguf_file::Content,
    filename: &str,
) -> Result<Tokenizer, ModelError> {
    if let Some(tokenizer) = load_sidecar_tokenizer(model_path) {
        return Ok(tokenizer);
    }

    match Tokenizer::from_gguf(content) {
        Ok(tokenizer) => Ok(tokenizer),
        Err(error) => {
            let model_kind = tokenizer_model_kind(content).unwrap_or_else(|| "unknown".to_string());
            if model_kind == "llama" {
                // Candle 0.11 only accepts `gpt2` here; some Phi GGUFs expose the same BPE metadata under `llama`.
                return build_bpe_tokenizer_from_gguf(content).map_err(|fallback_error| {
                    ModelError::InferenceFailed(format!(
                        "failed to build tokenizer from GGUF metadata for {filename}: {error}; llama-compat fallback failed: {fallback_error}"
                    ))
                });
            }

            Err(ModelError::InferenceFailed(format!(
                "failed to build tokenizer from GGUF metadata for {filename}: {error}"
            )))
        }
    }
}

fn load_sidecar_tokenizer(model_path: &Path) -> Option<Tokenizer> {
    let parent = model_path.parent()?;
    let stem = model_path.file_stem()?.to_string_lossy();

    let candidates = [
        parent.join(format!("{}.tokenizer.json", stem)),
        parent.join(format!("{}.json", stem)),
        parent.join("tokenizer.json"),
    ];

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        if let Ok(tokenizer) = Tokenizer::from_file(&candidate) {
            return Some(tokenizer);
        }
    }

    None
}

struct TokenizerPipeline {
    normalizer: Option<NormalizerWrapper>,
    pretokenizer: Option<PreTokenizerWrapper>,
    decoder: Option<DecoderWrapper>,
    post_processor: Option<PostProcessorWrapper>,
}

impl TokenizerPipeline {
    fn apply(self, tokenizer: &mut Tokenizer) {
        if let Some(normalizer) = self.normalizer {
            tokenizer.with_normalizer(Some(normalizer));
        }
        if let Some(pretokenizer) = self.pretokenizer {
            tokenizer.with_pre_tokenizer(Some(pretokenizer));
        }
        if let Some(decoder) = self.decoder {
            tokenizer.with_decoder(Some(decoder));
        }
        if let Some(post_processor) = self.post_processor {
            tokenizer.with_post_processor(Some(post_processor));
        }
    }
}

fn tokenizer_model_kind(content: &gguf_file::Content) -> Option<String> {
    content
        .metadata
        .get("tokenizer.ggml.model")
        .and_then(|value| value.to_string().ok())
        .map(|value| value.to_lowercase())
}

fn metadata_value<'a>(
    content: &'a gguf_file::Content,
    key: &str,
) -> Result<&'a gguf_file::Value, ModelError> {
    content.metadata.get(key).ok_or_else(|| {
        ModelError::InferenceFailed(format!("missing GGUF tokenizer metadata key `{key}`"))
    })
}

fn gguf_value_to_u32(value: &gguf_file::Value) -> Result<u32, ModelError> {
    use gguf_file::Value::*;
    match value {
        U8(v) => Ok(*v as u32),
        I8(v) => Ok(*v as u32),
        U16(v) => Ok(*v as u32),
        I16(v) => Ok(*v as u32),
        U32(v) => Ok(*v),
        I32(v) => Ok(*v as u32),
        U64(v) => Ok(*v as u32),
        I64(v) => Ok(*v as u32),
        _ => Err(ModelError::InferenceFailed(format!(
            "expected numeric tokenizer metadata value, got {value:?}"
        ))),
    }
}

fn gguf_value_to_f64(value: &gguf_file::Value) -> Result<f64, ModelError> {
    use gguf_file::Value::*;
    match value {
        F32(v) => Ok(*v as f64),
        F64(v) => Ok(*v),
        U8(v) => Ok(*v as f64),
        I8(v) => Ok(*v as f64),
        U16(v) => Ok(*v as f64),
        I16(v) => Ok(*v as f64),
        U32(v) => Ok(*v as f64),
        I32(v) => Ok(*v as f64),
        U64(v) => Ok(*v as f64),
        I64(v) => Ok(*v as f64),
        _ => Err(ModelError::InferenceFailed(format!(
            "expected numeric tokenizer score value, got {value:?}"
        ))),
    }
}

fn value_to_string_array(value: &gguf_file::Value, name: &str) -> Result<Vec<String>, ModelError> {
    let values = value.to_vec().map_err(|error| {
        ModelError::InferenceFailed(format!("`{name}` is not an array: {error}"))
    })?;

    values
        .iter()
        .map(|entry| {
            entry
                .to_string()
                .map(|value| value.to_string())
                .map_err(|error| {
                    ModelError::InferenceFailed(format!(
                        "`{name}` element is not a string ({entry:?}): {error}"
                    ))
                })
        })
        .collect()
}

fn merges_from_value(value: &gguf_file::Value) -> Result<Vec<(String, String)>, ModelError> {
    value_to_string_array(value, "tokenizer.ggml.merges")?
        .into_iter()
        .map(|merge| {
            merge.split_once(' ').map(|(left, right)| {
                (left.to_string(), right.to_string())
            }).ok_or_else(|| {
                ModelError::InferenceFailed(format!("invalid merge entry `{merge}`"))
            })
        })
        .collect()
}

fn pre_tokenizer_sequence(
    regex: &str,
    byte_level: ByteLevelPre,
) -> Result<PreTokenizerWrapper, ModelError> {
    let split = Split::new(
        SplitPattern::Regex(regex.to_string()),
        SplitDelimiterBehavior::Isolated,
        false,
    )
    .map_err(|error| ModelError::InferenceFailed(error.to_string()))?;

    Ok(Sequence::new(vec![split.into(), byte_level.into()]).into())
}

fn pipeline_from_pre(pre: &str) -> Result<TokenizerPipeline, ModelError> {
    const REGEX_QWEN2: &str = r"(?:'[sS]|'[tT]|'[rR][eE]|'[vV][eE]|'[mM]|'[lL][lL]|'[dD])|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+";
    const REGEX_LLAMA3: &str = r"(?:'[sS]|'[tT]|'[rR][eE]|'[vV][eE]|'[mM]|'[lL][lL]|'[dD])|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}{1,3}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+";

    Ok(match pre {
        "qwen2" => TokenizerPipeline {
            normalizer: Some(NFC.into()),
            pretokenizer: Some(pre_tokenizer_sequence(
                REGEX_QWEN2,
                ByteLevelPre::new(false, false, false),
            )?),
            decoder: Some(ByteLevelDecoder::new(false, false, false).into()),
            post_processor: Some(ByteLevelProcessor::new(false, false, false).into()),
        },
        "smaug-bpe" | "lfm2" | "llama3" => TokenizerPipeline {
            normalizer: None,
            pretokenizer: Some(pre_tokenizer_sequence(
                REGEX_LLAMA3,
                ByteLevelPre::new(false, true, false),
            )?),
            decoder: Some(ByteLevelDecoder::new(true, true, true).into()),
            post_processor: Some(ByteLevelProcessor::new(true, false, true).into()),
        },
        _ => TokenizerPipeline {
            normalizer: None,
            pretokenizer: Some(ByteLevelPre::default().into()),
            decoder: Some(ByteLevelDecoder::default().into()),
            post_processor: Some(ByteLevelProcessor::default().into()),
        },
    })
}

fn template_processor(
    tokens: &[String],
    bos_id: Option<u32>,
    eos_id: Option<u32>,
    add_bos: bool,
    add_eos: bool,
) -> Option<PostProcessorWrapper> {
    if (!add_bos && !add_eos) || tokens.is_empty() {
        return None;
    }

    let bos = bos_id.and_then(|id| tokens.get(id as usize)).cloned();
    let eos = eos_id.and_then(|id| tokens.get(id as usize)).cloned();

    let mut specials = Vec::new();
    if add_bos {
        let id = bos_id?;
        let token = bos.clone()?;
        specials.push((token.clone(), id));
    }
    if add_eos {
        let id = eos_id?;
        let token = eos.clone()?;
        specials.push((token.clone(), id));
    }

    let mut single = Vec::new();
    if add_bos {
        single.push(bos.clone()?);
    }
    single.push("$0".to_string());
    if add_eos {
        single.push(eos.clone()?);
    }

    let mut pair = Vec::new();
    if add_bos {
        pair.push(format!("{}:0", bos.clone()?));
    }
    pair.push("$A:0".to_string());
    if add_eos {
        pair.push(format!("{}:0", eos.clone()?));
    }
    if add_bos {
        pair.push(format!("{}:1", bos.clone()?));
    }
    pair.push("$B:1".to_string());
    if add_eos {
        pair.push(format!("{}:1", eos.clone()?));
    }

    let processor = tokenizers::processors::template::TemplateProcessing::builder()
        .try_single(single)
        .ok()?
        .try_pair(pair)
        .ok()?
        .special_tokens(specials)
        .build()
        .ok()?;

    Some(PostProcessorWrapper::Template(processor))
}

fn build_bpe_tokenizer_from_gguf(content: &gguf_file::Content) -> Result<Tokenizer, ModelError> {
    let model_kind = metadata_value(content, "tokenizer.ggml.model")?
        .to_string()
        .map_err(|error| ModelError::InferenceFailed(error.to_string()))?
        .to_lowercase();

    if model_kind != "gpt2" && model_kind != "llama" {
        return Err(ModelError::InferenceFailed(format!(
            "unsupported tokenizer model `{model_kind}`"
        )));
    }

    let tokens = value_to_string_array(
        metadata_value(content, "tokenizer.ggml.tokens")?,
        "tokenizer.ggml.tokens",
    )?;
    let vocab: Vocab = tokens
        .iter()
        .enumerate()
        .map(|(index, token)| (token.clone(), index as u32))
        .collect();
    let merges = match metadata_value(content, "tokenizer.ggml.merges") {
        Ok(value) => merges_from_value(value)?,
        Err(_) => {
            return build_unigram_tokenizer_from_gguf(content);
        }
    };

    let mut builder = BPE::builder().vocab_and_merges(vocab, merges);

    if let Ok(value) = metadata_value(content, "tokenizer.ggml.unk_token_id") {
        let token_id = gguf_value_to_u32(value)?;
        if let Some(token) = tokens.get(token_id as usize) {
            builder = builder.unk_token(token.clone());
        }
    }

    if let Ok(value) = metadata_value(content, "tokenizer.ggml.byte_fallback") {
        let byte_fallback = value
            .to_bool()
            .map_err(|error| ModelError::InferenceFailed(error.to_string()))?;
        builder = builder.byte_fallback(byte_fallback);
    }

    if let Ok(value) = metadata_value(content, "tokenizer.ggml.ignore_merges") {
        let ignore_merges = value
            .to_bool()
            .map_err(|error| ModelError::InferenceFailed(error.to_string()))?;
        builder = builder.ignore_merges(ignore_merges);
    }

    let bpe = builder
        .build()
        .map_err(|error| ModelError::InferenceFailed(error.to_string()))?;
    let mut tokenizer = Tokenizer::new(bpe);

    let pre = metadata_value(content, "tokenizer.ggml.pre")
        .and_then(|value| {
            value
                .to_string()
                .map(|pre| pre.to_string())
                .map_err(|error| ModelError::InferenceFailed(error.to_string()))
        })
        .unwrap_or_else(|_| "gpt2".to_string());
    let pipeline = pipeline_from_pre(pre.as_str())?;
    let post_processor_base = pipeline.post_processor.clone();

    let add_bos = metadata_value(content, "tokenizer.ggml.add_bos_token")
        .and_then(|value| {
            value
                .to_bool()
                .map_err(|error| ModelError::InferenceFailed(error.to_string()))
        })
        .unwrap_or(false);
    let add_eos = metadata_value(content, "tokenizer.ggml.add_eos_token")
        .and_then(|value| {
            value
                .to_bool()
                .map_err(|error| ModelError::InferenceFailed(error.to_string()))
        })
        .unwrap_or(false);
    let bos_id = metadata_value(content, "tokenizer.ggml.bos_token_id")
        .and_then(gguf_value_to_u32)
        .ok();
    let eos_id = metadata_value(content, "tokenizer.ggml.eos_token_id")
        .and_then(gguf_value_to_u32)
        .ok();

    pipeline.apply(&mut tokenizer);

    let template = template_processor(&tokens, bos_id, eos_id, add_bos, add_eos);
    if template.is_some() || post_processor_base.is_some() {
        let mut processors = Vec::new();
        if let Some(processor) = post_processor_base {
            processors.push(processor);
        }
        if let Some(processor) = template {
            processors.push(processor);
        }
        let post_processor = if processors.len() == 1 {
            processors.pop().expect("single processor exists")
        } else {
            ProcessorSequence::new(processors).into()
        };
        tokenizer.with_post_processor(Some(post_processor));
    }

    if let Ok(gguf_file::Value::Array(values)) = metadata_value(content, "tokenizer.ggml.token_type") {
        let mut special_tokens = Vec::new();
        for (index, value) in values.iter().enumerate() {
            let token_type = gguf_value_to_u32(value)?;
            let is_special = matches!(token_type, 2..=5);
            if is_special {
                if let Some(token) = tokens.get(index) {
                    special_tokens.push(AddedToken::from(token.clone(), true));
                }
            }
        }
        if !special_tokens.is_empty() {
            tokenizer.add_special_tokens(&special_tokens);
        }
    }

    let mut explicit_special_ids = HashSet::new();
    for key in [
        "tokenizer.ggml.bos_token_id",
        "tokenizer.ggml.eos_token_id",
        "tokenizer.ggml.pad_token_id",
        "tokenizer.ggml.sep_token_id",
        "tokenizer.ggml.unk_token_id",
    ] {
        if let Ok(value) = metadata_value(content, key) {
            explicit_special_ids.insert(gguf_value_to_u32(value)?);
        }
    }
    if !explicit_special_ids.is_empty() {
        let special_tokens: Vec<_> = explicit_special_ids
            .into_iter()
            .filter_map(|id| tokens.get(id as usize))
            .map(|token| AddedToken::from(token.clone(), true))
            .collect();
        if !special_tokens.is_empty() {
            tokenizer.add_special_tokens(&special_tokens);
        }
    }

    Ok(tokenizer)
}

fn build_unigram_tokenizer_from_gguf(content: &gguf_file::Content) -> Result<Tokenizer, ModelError> {
    let tokens = value_to_string_array(
        metadata_value(content, "tokenizer.ggml.tokens")?,
        "tokenizer.ggml.tokens",
    )?;

    let scores_value = metadata_value(content, "tokenizer.ggml.scores")?;
    let scores = scores_value.to_vec().map_err(|error| {
        ModelError::InferenceFailed(format!("`tokenizer.ggml.scores` is not an array: {error}"))
    })?;

    let pieces: Vec<(String, f64)> = tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            let score = scores
                .get(index)
                .map(gguf_value_to_f64)
                .transpose()?
                .unwrap_or(0.0);
            Ok((token.clone(), score))
        })
        .collect::<Result<Vec<_>, ModelError>>()?;

    let unk_id = metadata_value(content, "tokenizer.ggml.unknown_token_id")
        .or_else(|_| metadata_value(content, "tokenizer.ggml.unk_token_id"))
        .and_then(gguf_value_to_u32)
        .ok()
        .map(|id| id as usize);

    let model = Unigram::from(pieces, unk_id, false)
        .map_err(|error| ModelError::InferenceFailed(error.to_string()))?;
    let mut tokenizer = Tokenizer::new(model);

    let metaspace = Metaspace::new('▁', PrependScheme::Always, true);
    tokenizer.with_pre_tokenizer(Some(PreTokenizerWrapper::from(metaspace.clone())));
    tokenizer.with_decoder(Some(DecoderWrapper::from(MetaspaceDecoder::new(
        '▁',
        PrependScheme::Always,
        true,
    ))));

    let add_bos = metadata_value(content, "tokenizer.ggml.add_bos_token")
        .and_then(|value| {
            value
                .to_bool()
                .map_err(|error| ModelError::InferenceFailed(error.to_string()))
        })
        .unwrap_or(false);
    let add_eos = metadata_value(content, "tokenizer.ggml.add_eos_token")
        .and_then(|value| {
            value
                .to_bool()
                .map_err(|error| ModelError::InferenceFailed(error.to_string()))
        })
        .unwrap_or(false);
    let bos_id = metadata_value(content, "tokenizer.ggml.bos_token_id")
        .and_then(gguf_value_to_u32)
        .ok();
    let eos_id = metadata_value(content, "tokenizer.ggml.eos_token_id")
        .and_then(gguf_value_to_u32)
        .ok();

    if let Some(template) = template_processor(&tokens, bos_id, eos_id, add_bos, add_eos) {
        tokenizer.with_post_processor(Some(template));
    }

    let mut explicit_special_ids = HashSet::new();
    for key in [
        "tokenizer.ggml.bos_token_id",
        "tokenizer.ggml.eos_token_id",
        "tokenizer.ggml.pad_token_id",
        "tokenizer.ggml.padding_token_id",
        "tokenizer.ggml.sep_token_id",
        "tokenizer.ggml.unk_token_id",
        "tokenizer.ggml.unknown_token_id",
    ] {
        if let Ok(value) = metadata_value(content, key) {
            explicit_special_ids.insert(gguf_value_to_u32(value)?);
        }
    }
    if !explicit_special_ids.is_empty() {
        let special_tokens: Vec<_> = explicit_special_ids
            .into_iter()
            .filter_map(|id| tokens.get(id as usize))
            .map(|token| AddedToken::from(token.clone(), true))
            .collect();
        if !special_tokens.is_empty() {
            tokenizer.add_special_tokens(&special_tokens);
        }
    }

    Ok(tokenizer)
}

fn open_runtime_file(path: &Path) -> Result<File, ModelError> {
    File::open(path)
        .map_err(|e| ModelError::ImportFailed(format!("{} ({})", path.display(), e)))
}

fn eos_token_ids(tokenizer: &Tokenizer) -> Vec<u32> {
    ["<|im_end|>", "<|endoftext|>", "</s>", "<|eot_id|>", "<|end|>"]
        .into_iter()
        .filter_map(|token| tokenizer.token_to_id(token))
        .collect()
}
