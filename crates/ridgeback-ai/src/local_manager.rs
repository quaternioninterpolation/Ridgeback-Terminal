//! Local model lifecycle manager: download, detect, delete, and run inference.
//!
//! Manages HuggingFace model files on disk and hosts a background inference
//! server that communicates via channels.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use anyhow::{Context, Result};
use ridgeback_config::ai::LocalModelConfig;
use serde::{Deserialize, Serialize};

// ── Public status types ─────────────────────────────────────────────────

/// High-level status of the local model.
#[derive(Debug, Clone)]
pub enum LocalModelStatus {
    /// No model files found on disk.
    NotDownloaded,
    /// Download is in progress.
    Downloading {
        downloaded_bytes: u64,
        total_bytes: u64,
    },
    /// Model files exist on disk.
    Downloaded {
        /// ISO-8601 timestamp of when the download completed.
        date: String,
        /// Total size of model files in bytes.
        size_bytes: u64,
    },
    /// Model is being loaded (between Start click and Running).
    Starting,
    /// Model is loaded and ready for inference.
    Running,
    /// Something went wrong.
    Error(String),
}

/// Metadata written alongside model files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMeta {
    pub huggingface_url: String,
    pub downloaded_at: String,
    pub total_bytes: u64,
    pub files: Vec<String>,
}

// ── Shared inner state ──────────────────────────────────────────────────

struct Inner {
    status: LocalModelStatus,
    config: LocalModelConfig,
    /// Channel to send completion requests to the inference thread.
    inference_tx: Option<std::sync::mpsc::Sender<InferenceRequest>>,
    /// Handle to the inference background thread so we can join/stop it.
    inference_handle: Option<std::thread::JoinHandle<()>>,
    /// Flag to signal the download thread is tracked (thread detaches itself).
    _download_active: bool,
}

/// A request sent to the inference thread.
pub(crate) struct InferenceRequest {
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub stop: Option<Vec<String>>,
    pub response_tx: std::sync::mpsc::Sender<Result<String>>,
}

// ── Device detection ────────────────────────────────────────────────────

/// Detected device capability.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub label: String,
    pub recommended: bool,
}

/// Probe the system for available compute devices.
pub fn detect_devices() -> Vec<DeviceInfo> {
    let mut devices = vec![
        DeviceInfo {
            id: "cpu".to_string(),
            label: "CPU".to_string(),
            recommended: false,
        },
    ];

    #[cfg(target_os = "macos")]
    {
        // Metal is always available on macOS 10.11+
        devices.push(DeviceInfo {
            id: "metal".to_string(),
            label: "Metal GPU".to_string(),
            recommended: true,
        });
        // Mark CPU as not recommended when Metal is available
    }

    #[cfg(feature = "cuda")]
    {
        // Probe for CUDA — in practice candle will fail at load time
        // if no CUDA device is present, so we just advertise it.
        devices.push(DeviceInfo {
            id: "cuda".to_string(),
            label: "CUDA GPU".to_string(),
            recommended: true,
        });
    }

    // If no GPU was marked recommended, mark CPU as recommended
    if !devices.iter().any(|d| d.recommended) {
        if let Some(cpu) = devices.first_mut() {
            cpu.recommended = true;
        }
    }

    devices
}

/// Returns the best device id ("metal", "cuda", or "cpu").
pub fn best_device_id() -> String {
    detect_devices()
        .iter()
        .find(|d| d.recommended)
        .map(|d| d.id.clone())
        .unwrap_or_else(|| "cpu".to_string())
}

// ── LocalModelManager ───────────────────────────────────────────────────

/// Thread-safe manager for local AI model lifecycle.
#[derive(Clone)]
pub struct LocalModelManager {
    inner: Arc<Mutex<Inner>>,
    /// Atomic progress counters readable without locking the mutex.
    pub downloaded_bytes: Arc<AtomicU64>,
    pub total_bytes: Arc<AtomicU64>,
    pub download_active: Arc<AtomicBool>,
}

impl LocalModelManager {
    /// Create a new manager and auto-detect whether the model is already downloaded.
    pub fn new(config: &LocalModelConfig) -> Self {
        let mgr = Self {
            inner: Arc::new(Mutex::new(Inner {
                status: LocalModelStatus::NotDownloaded,
                config: config.clone(),
                inference_tx: None,
                inference_handle: None,
                _download_active: false,
            })),
            downloaded_bytes: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            download_active: Arc::new(AtomicBool::new(false)),
        };
        mgr.detect();
        mgr
    }

    // ── Query methods ───────────────────────────────────────────────────

    /// Current status snapshot.
    pub fn status(&self) -> LocalModelStatus {
        let inner = self.inner.lock().unwrap();
        // If download is active, compute live progress from atomics
        if self.download_active.load(Ordering::Relaxed) {
            return LocalModelStatus::Downloading {
                downloaded_bytes: self.downloaded_bytes.load(Ordering::Relaxed),
                total_bytes: self.total_bytes.load(Ordering::Relaxed),
            };
        }
        inner.status.clone()
    }

    /// Is the inference engine currently running?
    pub fn is_running(&self) -> bool {
        matches!(self.status(), LocalModelStatus::Running)
    }

    /// Is a download in progress?
    pub fn is_downloading(&self) -> bool {
        self.download_active.load(Ordering::Relaxed)
    }

    /// Download progress as (downloaded, total) bytes. Returns None if not downloading.
    pub fn download_progress(&self) -> Option<(u64, u64)> {
        if self.download_active.load(Ordering::Relaxed) {
            Some((
                self.downloaded_bytes.load(Ordering::Relaxed),
                self.total_bytes.load(Ordering::Relaxed),
            ))
        } else {
            None
        }
    }

    /// Update the config (e.g. when URL changes in settings) and re-detect.
    pub fn update_config(&self, config: &LocalModelConfig) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.config = config.clone();
        }
        self.detect();
    }

    // ── Detection ───────────────────────────────────────────────────────

    /// Check disk for existing model files and update status.
    pub fn detect(&self) {
        let mut inner = self.inner.lock().unwrap();
        let model_path = inner.config.model_path();

        if let Some(ref path) = model_path {
            let meta_path = path.join(".meta.json");
            if meta_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&meta_path) {
                    if let Ok(meta) = serde_json::from_str::<ModelMeta>(&content) {
                        inner.status = LocalModelStatus::Downloaded {
                            date: meta.downloaded_at,
                            size_bytes: meta.total_bytes,
                        };
                        return;
                    }
                }
            }
            // Check if directory exists with any files even without meta
            if path.exists() && std::fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false) {
                // Has files but no meta — treat as downloaded with unknown date
                let size = dir_size(path);
                inner.status = LocalModelStatus::Downloaded {
                    date: "unknown".to_string(),
                    size_bytes: size,
                };
                return;
            }
        }

        inner.status = LocalModelStatus::NotDownloaded;
    }

    // ── Download ────────────────────────────────────────────────────────

    /// Start downloading the model in the background.
    /// `repaint` is called periodically to refresh the UI.
    pub fn start_download(&self, repaint: impl Fn() + Send + Sync + 'static) {
        // Don't start if already downloading
        if self.download_active.load(Ordering::Relaxed) {
            return;
        }

        let config = {
            let inner = self.inner.lock().unwrap();
            inner.config.clone()
        };

        let repo_id = match config.repo_id() {
            Some(id) => id,
            None => {
                let mut inner = self.inner.lock().unwrap();
                inner.status = LocalModelStatus::Error("Invalid HuggingFace URL".to_string());
                return;
            }
        };

        let model_path = match config.model_path() {
            Some(p) => p,
            None => {
                let mut inner = self.inner.lock().unwrap();
                inner.status = LocalModelStatus::Error("Cannot determine model directory".to_string());
                return;
            }
        };

        self.download_active.store(true, Ordering::Relaxed);
        self.downloaded_bytes.store(0, Ordering::Relaxed);
        self.total_bytes.store(0, Ordering::Relaxed);

        let inner = Arc::clone(&self.inner);
        let downloaded_bytes = Arc::clone(&self.downloaded_bytes);
        let total_bytes_atomic = Arc::clone(&self.total_bytes);
        let download_active = Arc::clone(&self.download_active);
        let hf_url = config.huggingface_url.clone();

        // Spawn a dedicated thread with its own tokio runtime for the download.
        // This avoids requiring a tokio runtime in the caller (eframe/egui UI thread).
        std::thread::Builder::new()
            .name("local-ai-download".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Local AI: failed to create download runtime: {}", e);
                        download_active.store(false, Ordering::Relaxed);
                        let mut guard = inner.lock().unwrap();
                        guard.status = LocalModelStatus::Error(format!("Runtime error: {}", e));
                        repaint();
                        return;
                    }
                };

                let result = rt.block_on(download_model(
                    &repo_id,
                    &model_path,
                    &hf_url,
                    &downloaded_bytes,
                    &total_bytes_atomic,
                    &repaint,
                ));

                download_active.store(false, Ordering::Relaxed);

                let mut guard = inner.lock().unwrap();
                match result {
                    Ok(meta) => {
                        guard.status = LocalModelStatus::Downloaded {
                            date: meta.downloaded_at,
                            size_bytes: meta.total_bytes,
                        };
                    }
                    Err(e) => {
                        guard.status = LocalModelStatus::Error(format!("{:#}", e));
                    }
                }
                drop(guard);
                repaint();
            })
            .ok();
    }

    // ── Deletion ────────────────────────────────────────────────────────

    /// Delete downloaded model files.
    pub fn delete_model(&self) {
        let mut inner = self.inner.lock().unwrap();
        // Stop inference first if running
        inner.inference_tx = None; // dropping sender causes recv loop to end
        if let Some(handle) = inner.inference_handle.take() {
            let _ = handle.join();
        }

        if let Some(path) = inner.config.model_path() {
            let _ = std::fs::remove_dir_all(&path);
        }
        inner.status = LocalModelStatus::NotDownloaded;
    }

    // ── Inference ───────────────────────────────────────────────────────

    /// Start the local inference server on a background thread.
    pub fn start_inference(&self) {
        let mut inner = self.inner.lock().unwrap();

        // Already running?
        if inner.inference_tx.is_some() {
            return;
        }

        // Must be downloaded
        let model_path = match inner.config.model_path() {
            Some(p) if p.exists() => p,
            _ => {
                inner.status = LocalModelStatus::Error("Model not downloaded".to_string());
                return;
            }
        };

        let device_id = if inner.config.device == "auto" {
            best_device_id()
        } else {
            inner.config.device.clone()
        };
        let context_length = inner.config.context_length;

        let (tx, rx) = std::sync::mpsc::channel::<InferenceRequest>();
        inner.inference_tx = Some(tx);
        inner.status = LocalModelStatus::Starting;

        let inner_ref = Arc::clone(&self.inner);

        let handle = std::thread::Builder::new()
            .name("local-ai-inference".to_string())
            .spawn(move || {
                tracing::info!("Local AI: loading model from {:?} on device={}", model_path, device_id);

                let pipeline = match load_model(&model_path, &device_id, context_length) {
                    Ok(pipeline) => {
                        tracing::info!("Local AI: model loaded successfully");
                        pipeline
                    }
                    Err(e) => {
                        tracing::error!("Local AI: failed to load model: {:#}", e);
                        let mut guard = inner_ref.lock().unwrap();
                        guard.status = LocalModelStatus::Error(format!("Load failed: {:#}", e));
                        guard.inference_tx = None;
                        return;
                    }
                };

                // Mark as running
                {
                    let mut guard = inner_ref.lock().unwrap();
                    guard.status = LocalModelStatus::Running;
                }

                // Process inference requests (blocking recv on std::sync::mpsc)
                let mut pipeline = pipeline;
                tracing::info!("Local AI: inference thread ready, waiting for requests...");
                while let Ok(req) = rx.recv() {
                    tracing::info!("Local AI: received inference request, prompt_len={}, max_tokens={}", req.prompt.len(), req.max_tokens);
                    let start = std::time::Instant::now();
                    let result = pipeline.generate(
                        &req.prompt,
                        req.max_tokens,
                        req.temperature,
                        req.stop.as_deref(),
                    );
                    let elapsed = start.elapsed();
                    match &result {
                        Ok(text) => tracing::info!("Local AI: generation completed in {:.1}s, output_len={}", elapsed.as_secs_f64(), text.len()),
                        Err(e) => tracing::error!("Local AI: generation failed after {:.1}s: {:#}", elapsed.as_secs_f64(), e),
                    }
                    let _ = req.response_tx.send(result);
                }

                // Channel closed — clean up
                let mut guard = inner_ref.lock().unwrap();
                if matches!(guard.status, LocalModelStatus::Running) {
                    if let Some(ref path) = guard.config.model_path() {
                        let meta_path = path.join(".meta.json");
                        if let Ok(content) = std::fs::read_to_string(&meta_path) {
                            if let Ok(meta) = serde_json::from_str::<ModelMeta>(&content) {
                                guard.status = LocalModelStatus::Downloaded {
                                    date: meta.downloaded_at,
                                    size_bytes: meta.total_bytes,
                                };
                                return;
                            }
                        }
                    }
                    guard.status = LocalModelStatus::NotDownloaded;
                }
            })
            .ok();

        inner.inference_handle = handle;
    }

    /// Stop the inference server.
    pub fn stop_inference(&self) {
        let mut inner = self.inner.lock().unwrap();
        // Drop the sender — this will cause the recv loop to end
        inner.inference_tx = None;
        // Give the thread a moment to notice the channel closed, then join
        if let Some(handle) = inner.inference_handle.take() {
            drop(inner); // release lock before join
            let _ = handle.join();
        } else {
            drop(inner);
        }
        self.detect();
    }

    /// Send a completion request to the running inference engine.
    /// This is blocking — it sends the request and waits for the response.
    /// Applies a timeout of 5 minutes to prevent indefinite hangs.
    pub fn complete_blocking(&self, prompt: String, max_tokens: u32, temperature: f32, stop: Option<Vec<String>>) -> Result<String> {
        // Cap max_tokens for local inference — generating 256 tokens on CPU is very slow
        let max_tokens = max_tokens.min(128);
        tracing::info!("Local AI: complete_blocking called, prompt length={}, max_tokens={}", prompt.len(), max_tokens);

        let tx = {
            let inner = self.inner.lock().unwrap();
            inner.inference_tx.clone()
                .ok_or_else(|| anyhow::anyhow!("Local model inference is not running"))?
        };

        let (response_tx, response_rx) = std::sync::mpsc::channel();
        tracing::info!("Local AI: sending inference request to background thread...");
        tx.send(InferenceRequest {
            prompt,
            max_tokens,
            temperature,
            stop,
            response_tx,
        }).map_err(|_| anyhow::anyhow!("Inference channel closed"))?;

        tracing::info!("Local AI: waiting for inference response (timeout=5min)...");
        let result = response_rx.recv_timeout(std::time::Duration::from_secs(300))
            .map_err(|e| match e {
                std::sync::mpsc::RecvTimeoutError::Timeout => {
                    tracing::error!("Local AI: inference timed out after 5 minutes");
                    anyhow::anyhow!("Local model inference timed out (>5 min). The model may be too large for CPU inference. Try reducing max tokens or using a GPU.")
                }
                std::sync::mpsc::RecvTimeoutError::Disconnected => {
                    tracing::error!("Local AI: inference channel disconnected");
                    anyhow::anyhow!("Inference response channel closed unexpectedly")
                }
            })?;
        match &result {
            Ok(text) => tracing::info!("Local AI: got response, length={}", text.len()),
            Err(e) => tracing::error!("Local AI: inference error: {:#}", e),
        }
        result
    }
}

// ── Download implementation ─────────────────────────────────────────────

/// Files we need from the HuggingFace repo for a transformers model.
const REQUIRED_PATTERNS: &[&str] = &[
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "generation_config.json",
];

/// File entry from the HF API.
#[derive(Debug, Deserialize)]
struct HfFileEntry {
    path: String,
    size: Option<u64>,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    /// LFS metadata — present for large files stored via Git LFS.
    lfs: Option<HfLfsInfo>,
}

#[derive(Debug, Deserialize)]
struct HfLfsInfo {
    size: u64,
}

/// List files in a HuggingFace repo via the API.
async fn list_hf_files(repo_id: &str) -> Result<Vec<HfFileEntry>> {
    let url = format!("https://huggingface.co/api/models/{}/tree/main", repo_id);
    let client = reqwest::Client::new();
    let resp = client.get(&url)
        .header("User-Agent", "ridgeback-terminal/1.0")
        .send().await
        .context("Failed to contact HuggingFace API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HuggingFace API error {}: {}", status, body);
    }

    let files: Vec<HfFileEntry> = resp.json().await
        .context("Failed to parse HuggingFace file listing")?;
    Ok(files)
}

/// Determine which files to download from the repo.
fn select_download_files(files: &[HfFileEntry]) -> Vec<&HfFileEntry> {
    files.iter().filter(|f| {
        // Skip directories
        if f.entry_type.as_deref() == Some("directory") {
            return false;
        }
        let name = &f.path;
        // Always download required config/tokenizer files
        if REQUIRED_PATTERNS.iter().any(|p| name.ends_with(p)) {
            return true;
        }
        // Download safetensors model files
        if name.ends_with(".safetensors") {
            return true;
        }
        // Download model.safetensors.index.json if present
        if name.ends_with(".safetensors.index.json") {
            return true;
        }
        // Download special_tokens_map.json, vocab files, merges
        if name.ends_with("special_tokens_map.json")
            || name.ends_with("vocab.json")
            || name.ends_with("merges.txt")
        {
            return true;
        }
        false
    }).collect()
}

/// Download model files from HuggingFace.
async fn download_model(
    repo_id: &str,
    model_path: &PathBuf,
    hf_url: &str,
    downloaded_bytes: &Arc<AtomicU64>,
    total_bytes: &Arc<AtomicU64>,
    repaint: &(impl Fn() + Send),
) -> Result<ModelMeta> {
    // List files
    let all_files = list_hf_files(repo_id).await?;
    let to_download = select_download_files(&all_files);

    if to_download.is_empty() {
        anyhow::bail!("No downloadable model files found in repository");
    }

    // Compute total size (prefer lfs.size for LFS-tracked files)
    let total: u64 = to_download.iter().map(|f| {
        f.lfs.as_ref().map(|l| l.size).or(f.size).unwrap_or(0)
    }).sum();
    total_bytes.store(total, Ordering::Relaxed);
    repaint();

    // Create model directory
    std::fs::create_dir_all(model_path)
        .with_context(|| format!("Failed to create model directory: {:?}", model_path))?;

    let client = reqwest::Client::new();
    let mut file_names = Vec::new();

    for file_entry in &to_download {
        let file_url = format!(
            "https://huggingface.co/{}/resolve/main/{}",
            repo_id, file_entry.path
        );

        let file_path = model_path.join(&file_entry.path);

        // Create subdirectories if needed
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        tracing::info!("Downloading: {}", file_entry.path);

        let resp = client.get(&file_url)
            .header("User-Agent", "ridgeback-terminal/1.0")
            .send().await
            .with_context(|| format!("Failed to download {}", file_entry.path))?;

        if !resp.status().is_success() {
            let status = resp.status();
            anyhow::bail!("Download failed for {} — HTTP {}", file_entry.path, status);
        }

        let mut file = std::fs::File::create(&file_path)
            .with_context(|| format!("Failed to create file {:?}", file_path))?;

        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        use std::io::Write;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error reading download stream")?;
            file.write_all(&chunk)?;
            downloaded_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);

            // Repaint every ~256KB to avoid excessive UI updates
            let current = downloaded_bytes.load(Ordering::Relaxed);
            if current % (256 * 1024) < chunk.len() as u64 || current == total {
                repaint();
            }
        }

        file_names.push(file_entry.path.clone());
    }

    // Write metadata
    let now = chrono::Utc::now().to_rfc3339();
    let meta = ModelMeta {
        huggingface_url: hf_url.to_string(),
        downloaded_at: now,
        total_bytes: downloaded_bytes.load(Ordering::Relaxed),
        files: file_names,
    };
    let meta_json = serde_json::to_string_pretty(&meta)?;
    std::fs::write(model_path.join(".meta.json"), meta_json)?;

    Ok(meta)
}

// ── Model loading & inference ───────────────────────────────────────────

/// An opaque wrapper around the loaded model pipeline.
struct ModelPipeline {
    model: candle_transformers::models::qwen2::ModelForCausalLM,
    tokenizer: tokenizers::Tokenizer,
    device: candle_core::Device,
}

impl ModelPipeline {
    fn generate(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        stop: Option<&[String]>,
    ) -> Result<String> {
        use candle_core::Tensor;
        use candle_transformers::generation::LogitsProcessor;

        tracing::debug!("Local AI generate: encoding prompt ({} chars)...", prompt.len());
        let encoding = self.tokenizer.encode(prompt, true)
            .map_err(|e| anyhow::anyhow!("Tokenizer encode error: {}", e))?;
        let input_ids = encoding.get_ids();
        let input_len = input_ids.len();
        tracing::debug!("Local AI generate: prompt encoded to {} tokens", input_len);

        let input_tensor = Tensor::new(input_ids, &self.device)?.unsqueeze(0)?;

        let mut logits_processor = LogitsProcessor::new(
            rand_seed(),
            Some(temperature as f64),
            None, // top_p
        );

        let mut all_tokens: Vec<u32> = input_ids.to_vec();

        // Clear KV cache
        self.model.clear_kv_cache();

        // Process the prompt (prefill)
        tracing::debug!("Local AI generate: running prefill forward pass...");
        let prefill_start = std::time::Instant::now();
        let logits = self.model.forward(&input_tensor, 0)?;
        tracing::debug!("Local AI generate: prefill done in {:.1}s", prefill_start.elapsed().as_secs_f64());
        let logits = logits.squeeze(0)?;
        let last_logits = logits.get(logits.dim(0)? - 1)?;
        let mut current_token = logits_processor.sample(&last_logits)?;
        all_tokens.push(current_token);

        // Generate tokens one at a time
        tracing::debug!("Local AI generate: starting token generation (max {})", max_tokens);
        let gen_start = std::time::Instant::now();
        let mut tokens_generated = 1u32;
        for i in 0..max_tokens.saturating_sub(1) {
            let next_input = Tensor::new(&[current_token], &self.device)?.unsqueeze(0)?;
            let logits = self.model.forward(&next_input, input_len + i as usize + 1)?;
            let logits = logits.squeeze(0)?;
            let last_logits = logits.get(logits.dim(0)? - 1)?;
            let next_token = logits_processor.sample(&last_logits)?;

            // Check for EOS
            if let Some(eos_id) = self.tokenizer.token_to_id("<|endoftext|>") {
                if next_token == eos_id {
                    tracing::debug!("Local AI generate: hit <|endoftext|> EOS at token {}", tokens_generated);
                    break;
                }
            }
            if let Some(eos_id) = self.tokenizer.token_to_id("<|im_end|>") {
                if next_token == eos_id {
                    tracing::debug!("Local AI generate: hit <|im_end|> EOS at token {}", tokens_generated);
                    break;
                }
            }

            all_tokens.push(next_token);
            current_token = next_token;
            tokens_generated += 1;

            // Log progress every 10 tokens
            if tokens_generated % 10 == 0 {
                let elapsed = gen_start.elapsed().as_secs_f64();
                let tps = tokens_generated as f64 / elapsed;
                tracing::debug!("Local AI generate: {} tokens in {:.1}s ({:.1} tok/s)", tokens_generated, elapsed, tps);
            }

            // Decode only the generated portion
            let decoded = self.tokenizer.decode(&all_tokens[input_len..], true)
                .map_err(|e| anyhow::anyhow!("Tokenizer decode error: {}", e))?;

            // Check stop sequences
            if let Some(stops) = stop {
                if stops.iter().any(|s| decoded.contains(s)) {
                    tracing::debug!("Local AI generate: hit stop sequence at token {}", tokens_generated);
                    let mut generated_text = decoded;
                    for s in stops {
                        if let Some(pos) = generated_text.find(s) {
                            generated_text.truncate(pos);
                        }
                    }
                    return Ok(generated_text);
                }
            }
        }

        let total_elapsed = gen_start.elapsed().as_secs_f64();
        let tps = if total_elapsed > 0.0 { tokens_generated as f64 / total_elapsed } else { 0.0 };
        tracing::info!("Local AI generate: done — {} tokens in {:.1}s ({:.1} tok/s)", tokens_generated, total_elapsed, tps);

        // Final decode
        let generated_text = self.tokenizer.decode(&all_tokens[input_len..], true)
            .map_err(|e| anyhow::anyhow!("Tokenizer decode error: {}", e))?;

        Ok(generated_text)
    }
}

/// Load a Qwen2 model from disk.
fn load_model(model_path: &PathBuf, device_id: &str, _context_length: u32) -> Result<ModelPipeline> {
    use candle_core::DType;

    let device = match device_id {
        #[cfg(target_os = "macos")]
        "metal" => candle_core::Device::new_metal(0)
            .unwrap_or_else(|_| candle_core::Device::Cpu),
        #[cfg(feature = "cuda")]
        "cuda" => candle_core::Device::new_cuda(0)
            .unwrap_or_else(|_| candle_core::Device::Cpu),
        _ => candle_core::Device::Cpu,
    };

    tracing::info!("Local AI: using device {:?}", device);

    // Load tokenizer
    let tokenizer_path = model_path.join("tokenizer.json");
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    // Load model config
    let config_path = model_path.join("config.json");
    let config_str = std::fs::read_to_string(&config_path)
        .context("Failed to read model config.json")?;
    let config: candle_transformers::models::qwen2::Config = serde_json::from_str(&config_str)
        .context("Failed to parse model config.json")?;

    // Load model weights from safetensors
    let safetensor_files = find_safetensor_files(model_path)?;
    if safetensor_files.is_empty() {
        anyhow::bail!("No .safetensors files found in {:?}", model_path);
    }

    let vb = unsafe {
        candle_nn::VarBuilder::from_mmaped_safetensors(&safetensor_files, DType::F32, &device)?
    };

    let model = candle_transformers::models::qwen2::ModelForCausalLM::new(&config, vb)?;

    Ok(ModelPipeline {
        model,
        tokenizer,
        device,
    })
}

/// Find all .safetensors files in a directory.
fn find_safetensor_files(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("safetensors") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Compute total size of a directory recursively.
fn dir_size(path: &PathBuf) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Simple random seed for sampling.
fn rand_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42)
}













