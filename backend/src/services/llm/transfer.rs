//! The parts of Ollama's API that move model data around — pulling from a registry and
//! importing a local file — as opposed to the chat/generate calls in `llm.rs`. Split out
//! because they share none of those calls' assumptions: they run for minutes to hours
//! (so they override the client's generation-sized timeout), and they report progress as
//! they go rather than returning one result.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::TryStreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

use super::{OllamaErrors, OllamaService};

/// Transfers get a day: a large model over a slow link legitimately takes hours, and the
/// point of the cap is only that a hung connection eventually ends.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// One line of `/api/pull`'s progress stream. Layers report `total`/`completed` in bytes
/// under their own `digest`; the other lines are phase names ("pulling manifest",
/// "verifying sha256 digest", "success").
#[derive(Deserialize, Default)]
pub struct PullEvent {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

impl OllamaService {
    /// Pulls `name` (a library tag like `qwen3:8b`, or `hf.co/<user>/<repo>[:<quant>]` for a
    /// GGUF on Hugging Face) into Ollama, calling `on_event` for every progress line. Returns
    /// once Ollama reports success; an error line from Ollama comes back as `Err` with its text.
    pub async fn pull_streaming(&self, name: &str, mut on_event: impl FnMut(PullEvent)) -> Result<(), OllamaErrors> {
        let mut res = self
            .client
            .post(format!("{}/api/pull", self.base_url))
            .timeout(TRANSFER_TIMEOUT)
            .json(&serde_json::json!({ "model": name, "stream": true }))
            .send()
            .await
            .map_err(|e| OllamaErrors::RequestFailed(e.to_string()))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/pull returned a non-success status");
            return Err(OllamaErrors::Rejected(status, error_text(&body)));
        }

        // The body is newline-delimited JSON, but chunk boundaries fall anywhere — a line can
        // arrive split across two chunks — so buffer until a newline completes one.
        let mut buffer = Vec::new();
        let mut succeeded = false;
        while let Some(chunk) = res.chunk().await.map_err(|e| OllamaErrors::RequestFailed(e.to_string()))? {
            buffer.extend_from_slice(&chunk);
            while let Some(newline) = buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buffer.drain(..=newline).collect();
                let Ok(event) = serde_json::from_slice::<PullEvent>(&line) else { continue };
                if let Some(error) = event.error {
                    return Err(OllamaErrors::Failed(error));
                }
                succeeded |= event.status == "success";
                on_event(event);
            }
        }

        if succeeded {
            Ok(())
        } else {
            Err(OllamaErrors::Failed("the pull ended before Ollama reported success".to_string()))
        }
    }

    /// Whether Ollama already holds the blob with this digest (`sha256:<hex>`).
    async fn blob_exists(&self, digest: &str) -> Result<bool, OllamaErrors> {
        let res = self
            .client
            .head(format!("{}/api/blobs/{digest}", self.base_url))
            .send()
            .await
            .map_err(|e| OllamaErrors::RequestFailed(e.to_string()))?;
        Ok(res.status().is_success())
    }

    /// Uploads a file as the blob `digest`, counting bytes sent into `sent` as they go.
    /// Skipped when Ollama already has it (importing the same file twice is then instant).
    async fn upload_blob(&self, digest: &str, path: &Path, sent: Arc<AtomicU64>) -> Result<(), OllamaErrors> {
        if self.blob_exists(digest).await? {
            let size = tokio::fs::metadata(path).await.map(|m| m.len()).unwrap_or(0);
            sent.store(size, Ordering::Relaxed);
            return Ok(());
        }

        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| OllamaErrors::Failed(format!("could not open {}: {e}", path.display())))?;
        let counted = ReaderStream::with_capacity(file, 1 << 20).inspect_ok(move |chunk| {
            sent.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        });

        let res = self
            .client
            .post(format!("{}/api/blobs/{digest}", self.base_url))
            .timeout(TRANSFER_TIMEOUT)
            .body(reqwest::Body::wrap_stream(counted))
            .send()
            .await
            .map_err(|e| OllamaErrors::RequestFailed(e.to_string()))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama blob upload returned a non-success status");
            return Err(OllamaErrors::Rejected(status, error_text(&body)));
        }
        Ok(())
    }

    /// Imports local GGUF files as a new Ollama model called `name` — what `ollama create`
    /// does, over the HTTP API: each file is hashed, uploaded as a blob, and the model is then
    /// created from the blobs. `files` are `(path, is_projector)`; a projector (`mmproj`) file
    /// gives the model vision. `progress` reports which file and step (hashing, uploading,
    /// creating) it's on, and how many of that file's bytes are done.
    pub async fn import_files(
        &self,
        name: &str,
        files: &[std::path::PathBuf],
        progress: &ImportProgress,
    ) -> Result<(), OllamaErrors> {
        let mut blobs: HashMap<String, String> = HashMap::new();

        for (index, path) in files.iter().enumerate() {
            progress.file_index.store(index, Ordering::Relaxed);
            progress.set_phase("hashing");
            let digest = sha256_file(path, progress.done.clone()).await.map_err(|e| {
                OllamaErrors::Failed(format!("could not read {}: {e}", path.display()))
            })?;
            let digest = format!("sha256:{digest}");

            progress.set_phase("uploading");
            self.upload_blob(&digest, path, progress.done.clone()).await?;

            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("model.gguf").to_string();
            blobs.insert(file_name, digest);
        }

        progress.set_phase("creating");
        let res = self
            .client
            .post(format!("{}/api/create", self.base_url))
            .timeout(TRANSFER_TIMEOUT)
            .json(&serde_json::json!({ "model": name, "files": blobs, "stream": false }))
            .send()
            .await
            .map_err(|e| OllamaErrors::RequestFailed(e.to_string()))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/create returned a non-success status");
            return Err(OllamaErrors::Rejected(status, error_text(&body)));
        }
        Ok(())
    }
}

/// Where an import currently is, shared between the task running it and whoever reports on it.
/// `done` restarts from zero at every step, so it reads as "bytes of this file, this step".
pub struct ImportProgress {
    pub done: Arc<AtomicU64>,
    /// Index into the file list of the file being handled.
    pub file_index: std::sync::atomic::AtomicUsize,
    phase: std::sync::Mutex<&'static str>,
}

impl ImportProgress {
    pub fn new() -> Self {
        Self {
            done: Arc::new(AtomicU64::new(0)),
            file_index: std::sync::atomic::AtomicUsize::new(0),
            phase: std::sync::Mutex::new("starting"),
        }
    }

    pub fn set_phase(&self, phase: &'static str) {
        *self.phase.lock().unwrap() = phase;
        self.done.store(0, Ordering::Relaxed);
    }

    pub fn phase(&self) -> &'static str {
        *self.phase.lock().unwrap()
    }
}

/// The `error` field of Ollama's JSON error body, or the raw text if it isn't one.
fn error_text(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str().map(str::to_string)))
        .unwrap_or_else(|| body.trim().to_string())
}

/// Hex SHA-256 of a file, with `done` counting bytes as it goes.
async fn sha256_file(path: &Path, done: Arc<AtomicU64>) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        // Hashing 1 MiB takes a couple of milliseconds — short enough to do inline between
        // awaits without stalling the runtime.
        hasher.update(&buffer[..read]);
        done.fetch_add(read as u64, Ordering::Relaxed);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
