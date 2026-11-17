//! Embedding backend for semantic search
//!
//! Built-in ONNX Runtime with paraphrase-multilingual-MiniLM-L12-v2.
//! 384-dim, INT8 quantized, auto-downloads on first use.
//! Falls back to ollama if ONNX fails.
//!
//! The model is a snapshot: one fixed revision, never followed upstream (Jack, 16 Nov
//! 2026: "we snapshot the onnx. no need to update it. we find a way to validate it").
//! Geniuz carries the SHA-256 of each file and loads nothing that doesn't match, so the
//! signed program vouches for the model it runs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use ort::value::Tensor;

/// The snapshot's revision on Hugging Face. A revision is a commit: what it names
/// cannot change, unlike `main`.
const HF_BASE_URL: &str = "https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2/resolve/e8f8c211226b894fcb81acc59f3b34ba3efd5f42";
const EMBEDDING_DIM: usize = 384;

/// One file of the snapshot: where it is fetched from, and what it must be.
#[derive(Clone, Copy)]
pub(crate) struct Pinned {
    pub remote: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

fn pinned_model() -> Pinned {
    if cfg!(target_arch = "aarch64") {
        Pinned {
            remote: "onnx/model_qint8_arm64.onnx",
            sha256: "783fea82d71a58179b830a4dbd2d58447e640609e98eedf9ffa12622d375a672",
            bytes: 118_412_398,
        }
    } else {
        Pinned {
            remote: "onnx/model_quint8_avx2.onnx",
            sha256: "98a01d88b7de996cdea58c32ca71208c09968d143798814b2ea09d3439dc334f",
            bytes: 118_453_870,
        }
    }
}

const PINNED_TOKENIZER: Pinned = Pinned {
    remote: "tokenizer.json",
    sha256: "2c3387be76557bd40970cec13153b3bbf80407865484b209e655e5e4729076b8",
    bytes: 9_081_518,
};

fn default_models_dir() -> PathBuf {
    crate::data_dir().join("models")
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

        // Said once, the first time the model arrives; every later load is quiet, so a
        // keep prints only what it kept.
        let first_time = !model_path.exists();
        ensure_verified(&model_path, pinned_model(), "model", HF_BASE_URL)?;
        ensure_verified(&tokenizer_path, PINNED_TOKENIZER, "tokenizer", HF_BASE_URL)?;

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

        if first_time {
            eprintln!("[geniuz] ✨ Search by meaning is ready. From now on, memories are found by what they mean, not only by their words.");
        }

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

/// SHA-256 of a file, as lowercase hex.
pub(crate) fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("couldn't open {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|e| format!("couldn't read {}: {}", path.display(), e))?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

/// Where the note that a file was checked is kept: beside the file.
fn checked_note(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".verified");
    path.with_file_name(name)
}

/// The file's size and modification time, as written in the note: a change in either
/// means the file is hashed again.
fn stamp(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{} {}.{:09}", meta.len(), modified.as_secs(), modified.subsec_nanos()))
}

/// Whether only this user can put files in `dir`. Elsewhere, a note that a file was
/// checked is worth nothing: whoever can swap the file can also restore its size and time.
fn only_we_can_write(dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        matches!(std::fs::metadata(dir), Ok(m) if m.uid() == unsafe { libc::geteuid() } && m.permissions().mode() & 0o022 == 0)
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        true
    }
}

/// Write the note that a file was checked, never through a link someone left in its place.
fn write_checked_note(note: &Path, text: &str) {
    use std::io::Write;
    let _ = std::fs::remove_file(note);
    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(note) {
        let _ = f.write_all(text.as_bytes());
    }
}

/// Whether the file at `path` is the pinned one. Hashing 118 MB on every load would
/// slow each keep, so a match is written down with the file's size and time, and the
/// file is hashed again only when those change. That shortcut is taken only in a
/// directory nobody else can write to; anywhere else the file is hashed every time.
/// (Where only its owner can write, anything able to swap the file could as easily
/// patch the program.)
pub(crate) fn is_pinned(path: &Path, pinned: Pinned) -> Result<bool, String> {
    let now = stamp(path).ok_or_else(|| format!("couldn't read {}", path.display()))?;
    let note = checked_note(path);
    let expected = format!("{} {}", pinned.sha256, now);
    let trust_note = path.parent().map(only_we_can_write).unwrap_or(false);
    if trust_note && std::fs::read_to_string(&note).map(|s| s.trim() == expected).unwrap_or(false) {
        return Ok(true);
    }
    let matches = std::fs::metadata(path).map(|m| m.len() == pinned.bytes).unwrap_or(false)
        && sha256_file(path)? == pinned.sha256;
    if matches {
        if trust_note {
            write_checked_note(&note, &format!("{}\n", expected));
        }
    } else {
        let _ = std::fs::remove_file(&note);
    }
    Ok(matches)
}

/// Make sure `path` holds the pinned file: check the one that is there, or fetch it.
/// A file that doesn't match is never loaded. It is set aside, not deleted, and the
/// pinned one is fetched in its place.
pub(crate) fn ensure_verified(path: &Path, pinned: Pinned, what: &str, base_url: &str) -> Result<(), String> {
    if path.exists() {
        if is_pinned(path, pinned)? {
            return Ok(());
        }
        let aside = path.with_extension(format!("unverified-{}", chrono::Utc::now().format("%Y%m%dT%H%M%SZ")));
        std::fs::rename(path, &aside).map_err(|e| format!(
            "The search {} at {} is not the one Geniuz ships with, and it couldn't be set aside ({}). It was not loaded.",
            what, path.display(), e
        ))?;
        eprintln!(
            "[geniuz] The search {} at {} was not the one Geniuz ships with. It was not loaded, and is kept as {}.",
            what, path.display(), aside.display()
        );
    }
    let url = format!("{}/{}", base_url, pinned.remote);
    eprintln!("[geniuz] Downloading the search {} from {}...", what, url);
    download_verified(&url, path, pinned)?;
    eprintln!("[geniuz] Saved and checked: {}", path.display());
    Ok(())
}

/// Fetch `url` to `dest`, and put it in place only if it is exactly the pinned file.
/// Whatever goes wrong, no partial download is left behind.
fn download_verified(url: &str, dest: &Path, pinned: Pinned) -> Result<(), String> {
    let result = download_verified_inner(url, dest, pinned);
    if result.is_err() {
        let _ = std::fs::remove_file(dest.with_extension("tmp"));
    }
    result
}

fn download_verified_inner(url: &str, dest: &Path, pinned: Pinned) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .build()
        .new_agent();
    let mut response = agent.get(url).call()
        .map_err(|e| format!("Download failed: {}", e))?;
    let tmp_path = dest.with_extension("tmp");
    // Made fresh, never through a link someone left in its place.
    let _ = std::fs::remove_file(&tmp_path);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    let mut reader = response.body_mut().as_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed partway: {}", e))?;
        if n == 0 { break; }
        total += n as u64;
        // A file larger than the pinned one is already wrong; don't fill the disk finding out.
        if total > pinned.bytes {
            drop(file);
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("The download from {} is larger than the file Geniuz expects. It was discarded.", host_of(url)));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("Failed to write model file: {}", e))?;
    }
    drop(file);
    let got: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    if got != pinned.sha256 {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "The download from {} is not the file Geniuz expects (its checksum differs). It was discarded.",
            host_of(url)
        ));
    }
    std::fs::rename(&tmp_path, dest)
        .map_err(|e| format!("Failed to finalize download: {}", e))?;
    if let Some(now) = stamp(dest) {
        write_checked_note(&checked_note(dest), &format!("{} {}\n", pinned.sha256, now));
    }
    Ok(())
}

// =============================================================================
// Ollama fallback
// =============================================================================

pub struct OllamaBackend { url: String, model: String }

// Ollama's current embedding API is POST /api/embed with {model, input} and
// the response is {embeddings: [[...]]} — always a batched array, even for
// single input. The legacy /api/embeddings returned {embedding: [...]} for
// a {model, prompt} request. Newer Ollama versions (0.3+) may return 500
// on the legacy endpoint for some models, so we use the current API.
#[derive(Serialize)]
struct OllamaReq<'a> { model: &'a str, input: &'a str }
#[derive(Deserialize)]
struct OllamaResp { embeddings: Vec<Vec<f32>> }

impl OllamaBackend {
    pub fn new() -> Self {
        Self {
            url: std::env::var("GENIUZ_EMBED_URL")
                .unwrap_or_else(|_| "http://localhost:11434/api/embed".to_string()),
            model: std::env::var("GENIUZ_EMBED_MODEL")
                .unwrap_or_else(|_| "paraphrase-multilingual:278m".to_string()),
        }
    }
}

impl EmbeddingBackend for OllamaBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut response = ureq::post(&self.url)
            .send_json(&OllamaReq { model: &self.model, input: text.trim() })
            .map_err(|e| format!("Ollama request failed: {}", e))?;
        let resp: OllamaResp = response.body_mut().read_json()
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;
        let embedding = resp.embeddings.into_iter().next()
            .ok_or_else(|| "Ollama returned no embeddings".to_string())?;
        if embedding.is_empty() { return Err("Ollama returned empty embedding".to_string()); }
        Ok(embedding)
    }
    fn name(&self) -> &str { "ollama" }
}

// =============================================================================
// Backend selection + search
// =============================================================================

/// `scheme://host:port` of an address, without any user, password, path or query.
fn host_of(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if scheme.is_empty() { host.to_string() } else { format!("{}://{}", scheme, host) }
}

pub fn create_backend() -> Result<Box<dyn EmbeddingBackend>, String> {
    match BuiltinBackend::new() {
        Ok(b) => Ok(Box::new(b)),
        Err(e) => {
            eprintln!("[geniuz] The built-in search model couldn't load ({}); trying Ollama.", e);
            let backend = OllamaBackend::new();
            // Verify ollama is reachable before claiming ready
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(2)))
                .build()
                .new_agent();
            // Reachability probe targets /api/tags regardless of which embed
            // endpoint the user configured — handles both /api/embed (current)
            // and /api/embeddings (legacy) URL shapes.
            let tags_url = backend.url
                .replace("/api/embed", "/api/tags")
                .replace("/api/tagsdings", "/api/tags"); // fix over-replace on legacy URL
            match agent.get(&tags_url).call() {
                Ok(_) => Ok(Box::new(backend)),
                // Only the host is named: the configured address may carry a password
                // or token, and this message can end up in logs.
                Err(_) => Err(format!("Search by meaning is unavailable: Ollama didn't answer at {}", host_of(&tags_url))),
            }
        }
    }
}

#[cfg(test)]
mod host_tests {
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("geniuz-pin-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // "hello\n", so the expected value can be checked with `printf 'hello\n' | shasum -a 256`.
    const HELLO: super::Pinned = super::Pinned {
        remote: "hello.txt",
        sha256: "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03",
        bytes: 6,
    };

    #[test]
    fn a_file_is_pinned_only_while_it_is_exactly_the_snapshot() {
        let dir = scratch("exact");
        let file = dir.join("hello.txt");
        std::fs::write(&file, "hello\n").unwrap();
        assert_eq!(super::sha256_file(&file).unwrap(), HELLO.sha256);
        assert!(super::is_pinned(&file, HELLO).unwrap());
        assert!(dir.join("hello.txt.verified").exists(), "a match is written down");
        assert!(super::is_pinned(&file, HELLO).unwrap(), "and trusted again while size and time hold");

        // Same length, different bytes: the note must not vouch for it.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&file, "jello\n").unwrap();
        assert!(!super::is_pinned(&file, HELLO).unwrap());
        assert!(!dir.join("hello.txt.verified").exists(), "a mismatch clears the note");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_note_copied_from_elsewhere_does_not_vouch_for_a_wrong_file() {
        let dir = scratch("forged");
        let file = dir.join("hello.txt");
        std::fs::write(&file, "nope!\n").unwrap();
        // A note naming the right hash but the wrong size and time is ignored.
        std::fs::write(dir.join("hello.txt.verified"), format!("{} 6 1.000000000\n", HELLO.sha256)).unwrap();
        assert!(!super::is_pinned(&file, HELLO).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_file_is_set_aside_and_never_left_in_place() {
        let dir = scratch("aside");
        let file = dir.join("hello.txt");
        std::fs::write(&file, "not the snapshot").unwrap();
        // Nothing listens on port 9, so the fetch fails: the point is what happened to the file.
        let err = super::ensure_verified(&file, HELLO, "model", "http://127.0.0.1:9").unwrap_err();
        assert!(err.contains("Download failed"), "{}", err);
        assert!(!file.exists(), "the wrong file is not left where it would be loaded");
        let kept: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("unverified-")).collect();
        assert_eq!(kept.len(), 1, "it is kept, not deleted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_address_is_named_by_its_host_only() {
        assert_eq!(super::host_of("http://user:secret@gpu.lan:11434/api/tags?token=x"), "http://gpu.lan:11434");
        assert_eq!(super::host_of("http://localhost:11434/api/tags"), "http://localhost:11434");
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

pub struct ScoredMemory {
    pub memory_uuid: String,
    pub gist: String,
    pub created_at: String,
    pub score: f32,
}

pub fn semantic_search_cached(
    query: &str, cached: Vec<CachedEmbedding>, limit: usize,
) -> Result<Vec<ScoredMemory>, String> {
    let backend = create_backend()?;
    let query_embedding = backend.embed(query)?;
    Ok(rank_cached(&query_embedding, cached, limit))
}

/// Rank cached embeddings against an already-embedded query, best first.
pub fn rank_cached(
    query_embedding: &[f32], cached: Vec<CachedEmbedding>, limit: usize,
) -> Vec<ScoredMemory> {
    let mut scored: Vec<ScoredMemory> = cached.into_iter().map(|c| {
        let score = cosine_similarity(query_embedding, &c.embedding);
        ScoredMemory { memory_uuid: c.memory_uuid, gist: c.gist, created_at: c.created_at, score }
    }).collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
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
