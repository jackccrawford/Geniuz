//! Embedding backend for semantic search
//!
//! Built-in ONNX Runtime with paraphrase-multilingual-MiniLM-L12-v2.
//! 384-dim, INT8 quantized, auto-downloads on first use.
//! Falls back to ollama if ONNX fails.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ort::value::Tensor;

const HF_BASE_URL: &str = "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/main";
const EMBEDDING_DIM: usize = 384;

fn onnx_model_filename() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "onnx/model_qint8_arm64.onnx"
    } else {
        "onnx/model_quint8_avx2.onnx"
    }
}

fn default_models_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".geniuz").join("models")
}

pub trait EmbeddingBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
    fn name(&self) -> &str;
}

// =============================================================================
// Built-in ONNX backend
// =============================================================================

pub struct BuiltinBackend {
    session: Mutex<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
}

impl BuiltinBackend {
    pub fn new() -> Result<Self, String> {
        let models_dir = std::env::var("GENIUZ_MODELS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_models_dir());

        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models directory: {}", e))?;

        let model_path = models_dir.join("model.onnx");
        let tokenizer_path = models_dir.join("tokenizer.json");

        if !model_path.exists() {
            let url = format!("{}/{}", HF_BASE_URL, onnx_model_filename());
            eprintln!("[geniuz] Downloading model from {}...", url);
            download_file(&url, &model_path)?;
            eprintln!("[geniuz] Model saved to {}", model_path.display());
        }

        if !tokenizer_path.exists() {
            let url = format!("{}/tokenizer.json", HF_BASE_URL);
            eprintln!("[geniuz] Downloading tokenizer...");
            download_file(&url, &tokenizer_path)?;
        }

        let session = ort::session::Session::builder()
            .map_err(|e| format!("Failed to create ONNX session builder: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| format!("Failed to set thread count: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("Failed to load ONNX model: {}", e))?;

        let mut tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        // 512 tokens max (model supports it, default is 128)
        let truncation = tokenizers::TruncationParams {
            max_length: 512,
            strategy: tokenizers::TruncationStrategy::LongestFirst,
            ..Default::default()
        };
        tokenizer.with_truncation(Some(truncation))
            .map_err(|e| format!("Failed to set truncation: {}", e))?;

        eprintln!("[geniuz] Semantic search ready ({})", onnx_model_filename());

        Ok(Self { session: Mutex::new(session), tokenizer })
    }

    fn infer(&self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self.tokenizer.encode(text.trim(), true)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| m as i64).collect();
        let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&t| t as i64).collect();
        let seq_len = input_ids.len();

        let shape = vec![1i64, seq_len as i64];
        let input_ids_tensor = Tensor::from_array((shape.clone(), input_ids))
            .map_err(|e| format!("Failed to create input_ids tensor: {}", e))?;
        let attention_mask_tensor = Tensor::from_array((shape.clone(), attention_mask.clone()))
            .map_err(|e| format!("Failed to create attention_mask tensor: {}", e))?;
        let token_type_ids_tensor = Tensor::from_array((shape, token_type_ids))
            .map_err(|e| format!("Failed to create token_type_ids tensor: {}", e))?;

        let mut session = self.session.lock()
            .map_err(|e| format!("Session lock poisoned: {}", e))?;
        let outputs = session.run(ort::inputs! {
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        }).map_err(|e| format!("ONNX inference failed: {}", e))?;

        let output_value = &outputs[0];
        let (_output_shape, output_data) = output_value
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract output tensor: {}", e))?;

        let mut pooled = vec![0.0f32; EMBEDDING_DIM];
        let mut mask_sum = 0.0f32;
        for token_idx in 0..seq_len {
            let mask_val = attention_mask[token_idx] as f32;
            mask_sum += mask_val;
            let offset = token_idx * EMBEDDING_DIM;
            for dim in 0..EMBEDDING_DIM {
                pooled[dim] += output_data[offset + dim] * mask_val;
            }
        }
        if mask_sum > 0.0 {
            for dim in 0..EMBEDDING_DIM {
                pooled[dim] /= mask_sum;
            }
        }

        let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut pooled {
                *v /= norm;
            }
        }

        Ok(pooled)
    }
}

impl EmbeddingBackend for BuiltinBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> { self.infer(text) }
    fn name(&self) -> &str { "builtin" }
}

fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .build()
        .new_agent();
    let mut response = agent.get(url).call()
        .map_err(|e| format!("Download failed: {}", e))?;
    let tmp_path = dest.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file)
        .map_err(|e| format!("Failed to write model file: {}", e))?;
    std::fs::rename(&tmp_path, dest)
        .map_err(|e| format!("Failed to finalize download: {}", e))?;
    Ok(())
}

// =============================================================================
// Ollama fallback
// =============================================================================

pub struct OllamaBackend { url: String, model: String }

#[derive(Serialize)]
struct OllamaReq<'a> { model: &'a str, prompt: &'a str }
#[derive(Deserialize)]
struct OllamaResp { embedding: Vec<f32> }

impl OllamaBackend {
    pub fn new() -> Self {
        Self {
            url: std::env::var("GENIUZ_EMBED_URL")
                .unwrap_or_else(|_| "http://localhost:11434/api/embeddings".to_string()),
            model: std::env::var("GENIUZ_EMBED_MODEL")
                .unwrap_or_else(|_| "paraphrase-multilingual:278m".to_string()),
        }
    }
}

impl EmbeddingBackend for OllamaBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut response = ureq::post(&self.url)
            .send_json(&OllamaReq { model: &self.model, prompt: text.trim() })
            .map_err(|e| format!("Ollama request failed: {}", e))?;
        let resp: OllamaResp = response.body_mut().read_json()
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;
        if resp.embedding.is_empty() { return Err("Empty embedding".to_string()); }
        Ok(resp.embedding)
    }
    fn name(&self) -> &str { "ollama" }
}

// =============================================================================
// Backend selection + search
// =============================================================================

pub fn create_backend() -> Result<Box<dyn EmbeddingBackend>, String> {
    match BuiltinBackend::new() {
        Ok(b) => Ok(Box::new(b)),
        Err(e) => {
            eprintln!("[geniuz] ONNX failed ({}), falling back to ollama", e);
            let backend = OllamaBackend::new();
            // Verify ollama is reachable before claiming ready
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(2)))
                .build()
                .new_agent();
            let tags_url = backend.url.replace("/api/embeddings", "/api/tags");
            match agent.get(&tags_url).call() {
                Ok(_) => Ok(Box::new(backend)),
                Err(_) => Err("Neither built-in ONNX nor Ollama available for embeddings".to_string()),
            }
        }
    }
}

/// Cosine similarity — full computation, not dot-product shortcut.
/// The ONNX backend L2-normalizes, but the ollama fallback does not.
/// Full cosine is correct for both.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 { return 0.0; }
    dot / (mag_a * mag_b)
}

pub struct CachedEmbedding {
    pub memory_uuid: String,
    pub gist: String,
    pub created_at: String,
    pub embedding: Vec<f32>,
}

pub struct ScoredSignal {
    pub memory_uuid: String,
    pub gist: String,
    pub created_at: String,
    pub score: f32,
}

pub fn semantic_search_cached(
    query: &str, cached: Vec<CachedEmbedding>, limit: usize,
) -> Result<Vec<ScoredSignal>, String> {
    let backend = create_backend()?;
    let query_embedding = backend.embed(query)?;
    let mut scored: Vec<ScoredSignal> = cached.into_iter().map(|c| {
        let score = cosine_similarity(&query_embedding, &c.embedding);
        ScoredSignal { memory_uuid: c.memory_uuid, gist: c.gist, created_at: c.created_at, score }
    }).collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

/// Embed content — socket first, inline fallback.
/// If geniuz-embed is running, uses the warm session (~3ms).
/// Otherwise loads ONNX inline (~500ms). Either way, it works.
pub fn embed_content(text: &str) -> Result<Vec<f32>, String> {
    // Try socket (warm ONNX session, ~3ms)
    if let Some(emb) = crate::embed_client::embed_via_socket(text) {
        return Ok(emb);
    }
    // Fallback: load model inline (~500ms)
    let backend = create_backend()?;
    backend.embed(text)
}

pub fn model_id() -> &'static str { "paraphrase-multilingual-MiniLM-L12-v2-qint8" }

pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding { blob.extend_from_slice(&val.to_le_bytes()); }
    blob
}

pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>, String> {
    let expected_bytes = EMBEDDING_DIM * 4;
    if blob.len() != expected_bytes {
        return Err(format!(
            "Corrupted embedding: expected {} bytes ({}×4), got {}",
            expected_bytes, EMBEDDING_DIM, blob.len()
        ));
    }
    Ok(blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}
