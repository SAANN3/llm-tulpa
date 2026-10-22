//! Speech to text for voice input, on the CPU (the GPU is the language model's). The models are sherpa-onnx's, installed
//! from Settings into `stt_dir`; what is installed is a matter of files on disk. A live input is a session that takes
//! audio in chunks and answers each with the text so far (see `session`), so it needs nothing but ordinary requests.

mod benchmark;
mod catalog;
mod engine;
mod install;
mod session;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

pub use benchmark::{cores, SttBenchModel, SttBenchRun, SttBenchmark};
pub use catalog::ModelSpec;
use catalog::MODELS;
use engine::Engine;
pub use install::SttTask;
use session::{Job, Session, MAX_SAMPLES, SAMPLE_RATE};

use super::error::ErrorService;

/// How many live inputs may be open at once, over all users: each holds up to five minutes of audio and takes its turn
/// at the one recognizer
/// A recording is read while the caller waits only up to this long: a minute and a half of audio takes Whisper about
/// as long again to read, and the next step is a job
pub const WAIT_SAMPLES: usize = SAMPLE_RATE * 90;
/// A recording read as a job may be this long: two hours, about 460 MB as samples
pub const MAX_TASK_SAMPLES: usize = SAMPLE_RATE * 60 * 120;

const MAX_SESSIONS: usize = 8;
/// A session nobody has sent audio to for this long is dropped (a page that closed without saying so)
const SESSION_IDLE: Duration = Duration::from_secs(120);
/// How often forgotten sessions and an idle model are looked for
const SWEEP_EVERY: Duration = Duration::from_secs(30);
/// How long a finishing input waits for the read in progress
const FINISH_WAIT: Duration = Duration::from_secs(60);

/// One speech model as the Settings page lists it
#[derive(Serialize, ToSchema)]
pub struct SttModel {
    pub id: String,
    pub label: String,
    pub description: String,
    pub languages: String,
    /// The size of the download
    pub download_bytes: u64,
    /// Roughly what it takes of memory while loaded
    pub memory_bytes: u64,
    pub installed: bool,
    /// Its files take this much disk, once installed
    pub disk_bytes: Option<u64>,
    /// Loaded in memory now
    pub loaded: bool,
}

/// What the owner may tune about how the models run
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct SttTuning {
    /// CPU threads a model reads with
    pub threads: u32,
    /// Minutes of no use after which the loaded model is let go of; 0 keeps it for good
    pub idle_unload_minutes: u64,
}

/// A day: longer is the same as never for a model that is used now and then
const MAX_IDLE_MINUTES: u64 = 24 * 60;

impl SttTuning {
    pub fn validate(&self) -> Result<(), ErrorService> {
        if self.threads == 0 || self.threads > cores() {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, format!("threads must be 1 to {}", cores())));
        }
        if self.idle_unload_minutes > MAX_IDLE_MINUTES {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, format!("the idle time may be {MAX_IDLE_MINUTES} minutes at most")));
        }
        Ok(())
    }
}

#[derive(Serialize, ToSchema)]
pub struct SttStatus {
    pub models: Vec<SttModel>,
    /// The install going on, or the last one that ended
    pub task: Option<SttTask>,
    /// The measurement of the models' speed going on, or the last one that ended
    pub benchmark: Option<SttBenchmark>,
    pub tuning: SttTuning,
    /// How many cores this machine has
    pub cores: u32,
}

pub struct SttService {
    dir: PathBuf,
    tuning: Mutex<SttTuning>,
    http: reqwest::Client,
    /// Where the model archives are downloaded from (the sherpa-onnx release; a local server in the tests)
    download_base: String,
    task: Arc<Mutex<Option<SttTask>>>,
    benchmark: Arc<Mutex<Option<SttBenchmark>>>,
    engine: Arc<Engine>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

fn disk_bytes(dir: &std::path::Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()?.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

/// 16-bit little-endian samples as the floats the recognizer takes
pub fn pcm16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0).collect()
}

impl SttService {
    pub fn new(dir: PathBuf, threads: u32, idle_unload_minutes: u64) -> Arc<Self> {
        Self::with_download_base(dir, threads, idle_unload_minutes, install::DOWNLOAD_BASE.to_string())
    }

    fn with_download_base(dir: PathBuf, threads: u32, idle_unload_minutes: u64, download_base: String) -> Arc<Self> {
        let service = Arc::new(Self {
            dir,
            tuning: Mutex::new(SttTuning { threads: threads.clamp(1, cores()), idle_unload_minutes }),
            // A connection that hangs fails after these instead of leaving the install at a standstill; the download then tries again
            http: reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).read_timeout(Duration::from_secs(60)).build().unwrap_or_default(),
            download_base,
            task: Arc::new(Mutex::new(None)),
            benchmark: Arc::new(Mutex::new(None)),
            engine: Arc::new(Engine::default()),
            sessions: Mutex::new(HashMap::new()),
        });
        // Holds only a weak handle: the sweep ends with the service
        let weak = Arc::downgrade(&service);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SWEEP_EVERY).await;
                let Some(service) = weak.upgrade() else { return };
                service.sweep();
            }
        });
        service
    }

    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Session>>> {
        self.sessions.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn sweep(&self) {
        let mut sessions = self.sessions();
        sessions.retain(|_, s| s.touched.lock().map(|t| t.elapsed() < SESSION_IDLE).unwrap_or(false));
        let open = !sessions.is_empty();
        drop(sessions);
        let idle = Some(self.tuning().idle_unload_minutes).filter(|&m| m > 0).map(|m| Duration::from_secs(m * 60));
        if let Some(idle) = idle.filter(|_| !open) {
            if self.engine.unload_if_idle(idle) {
                tracing::info!("the speech model was unloaded after {} minutes of no use", idle.as_secs() / 60);
            }
        }
    }

    pub fn tuning(&self) -> SttTuning {
        self.tuning.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Applies new settings at once: the idle time from the next sweep, the thread count from the next read (a loaded
    /// model is built for one, so it is loaded again)
    pub fn set_tuning(&self, tuning: SttTuning) {
        *self.tuning.lock().unwrap_or_else(|p| p.into_inner()) = tuning;
    }

    pub fn task(&self) -> Option<SttTask> {
        self.task.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn status(&self) -> SttStatus {
        let loaded = self.engine.loaded_id();
        SttStatus {
            models: MODELS
                .iter()
                .map(|m| {
                    let installed = install::is_installed(&self.dir, m.id);
                    SttModel {
                        id: m.id.to_string(),
                        label: m.label.to_string(),
                        description: m.description.to_string(),
                        languages: m.languages.to_string(),
                        download_bytes: m.download_bytes,
                        memory_bytes: m.memory_bytes,
                        installed,
                        disk_bytes: installed.then(|| disk_bytes(&self.dir.join(m.id))),
                        loaded: loaded == Some(m.id),
                    }
                })
                .collect(),
            task: self.task(),
            benchmark: self.benchmark(),
            tuning: self.tuning(),
            cores: cores(),
        }
    }

    /// Takes a model away: unloaded, its folder deleted
    pub fn remove(&self, id: &str) -> Result<(), ErrorService> {
        let spec = catalog::find(id).ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, format!("there is no speech model '{id}'")))?;
        if self.task().is_some_and(|t| t.state == "running" && t.model == spec.id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, "that model is being installed"));
        }
        if self.sessions().values().any(|s| s.model.id == spec.id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, "that model is in use by a voice input"));
        }
        self.engine.unload(spec.id);
        std::fs::remove_dir_all(self.dir.join(spec.id)).map_err(|e| {
            tracing::warn!("could not remove the speech model {id}: {e}");
            ErrorService::internal("could not remove the model's files")
        })
    }

    /// The model a user's voice input runs on: the one they chose when it is installed, else the first installed
    pub fn resolve_model(&self, chosen: Option<&str>) -> Result<&'static ModelSpec, ErrorService> {
        let installed = |m: &&'static ModelSpec| install::is_installed(&self.dir, m.id);
        chosen
            .and_then(catalog::find)
            .filter(|m| installed(&m))
            .or_else(|| MODELS.iter().find(installed))
            .ok_or_else(|| ErrorService::new(StatusCode::CONFLICT, "no speech model is installed; the owner can install one in Settings, Voice"))
    }

    /// Reads `audio` (16 kHz mono) with `model`; blocking
    fn read_blocking(&self, model: &'static ModelSpec, language: &str, audio: &[f32]) -> Result<String, String> {
        let recognizer = self.engine.recognizer(model, self.dir.join(model.id), language, self.tuning().threads as i32)?;
        Ok(self.engine.read(&recognizer, audio))
    }

    async fn read(self: &Arc<Self>, model: &'static ModelSpec, language: String, audio: Vec<f32>) -> Result<String, ErrorService> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.read_blocking(model, &language, &audio))
            .await
            .map_err(|e| ErrorService::internal(format!("the speech reader stopped: {e}")))?
            .map_err(|e| {
                tracing::warn!("speech recognition failed: {e}");
                ErrorService::new(StatusCode::SERVICE_UNAVAILABLE, e)
            })
    }

    /// The text of a whole recording, read in pieces (see `session::pieces`) and joined
    pub async fn transcribe(self: &Arc<Self>, model: &'static ModelSpec, language: &str, audio: Vec<f32>) -> Result<String, ErrorService> {
        if audio.len() > MAX_SAMPLES {
            return Err(ErrorService::new(StatusCode::PAYLOAD_TOO_LARGE, "a recording may be five minutes long at most"));
        }
        if audio.len() < SAMPLE_RATE / 10 {
            return Ok(String::new());
        }
        let mut texts = Vec::new();
        for piece in session::pieces(&audio) {
            let text = self.read(model, language.to_string(), piece.to_vec()).await?;
            if !text.is_empty() {
                texts.push(text);
            }
        }
        Ok(texts.join(" "))
    }

    /// Reads a recording too long to wait for, piece by piece, and appends one line per piece to `log` as it goes: the
    /// stretch of the recording it covers, then its text. A last line says how it ended, so a reader of the log's end
    /// can tell a finished transcript from one still being written. Answers whether it ran to the end.
    pub async fn transcribe_to_log(self: &Arc<Self>, model: &'static ModelSpec, language: &str, audio: Vec<f32>, log: &Path) -> bool {
        let append = |line: String| async move {
            let mut file = tokio::fs::OpenOptions::new().append(true).create(true).open(log).await?;
            file.write_all(format!("{line}\n").as_bytes()).await
        };
        let mut at = 0;
        let mut words = 0;
        let pieces = session::pieces(&audio);
        let count = pieces.len();
        for piece in pieces {
            let text = match self.read(model, language.to_string(), piece.to_vec()).await {
                Ok(text) => text,
                Err(e) => {
                    let _ = append(format!("[stopped at {}: {}]", clock(at), e.message.unwrap_or_default())).await;
                    return false;
                }
            };
            words += text.split_whitespace().count();
            let line = format!("[{}-{}] {}", clock(at), clock(at + piece.len()), if text.is_empty() { "(nothing recognized)" } else { &text });
            at += piece.len();
            if let Err(e) = append(line).await {
                tracing::warn!("could not write to the transcription log {}: {e}", log.display());
                return false;
            }
        }
        append(format!("[done: {count} parts, {words} words]")).await.is_ok()
    }

    pub fn start_session(&self, user_id: i64, model: &'static ModelSpec, language: &str) -> Result<String, ErrorService> {
        let mut sessions = self.sessions();
        if sessions.len() >= MAX_SESSIONS {
            return Err(ErrorService::new(StatusCode::TOO_MANY_REQUESTS, "too many voice inputs are open at once"));
        }
        let id: String = (0..16).map(|_| format!("{:x}", rand::random::<u8>() % 16)).collect();
        sessions.insert(id.clone(), Arc::new(Session::new(user_id, model, language.to_string())));
        Ok(id)
    }

    fn session(&self, user_id: i64, id: &str) -> Result<Arc<Session>, ErrorService> {
        self.sessions()
            .get(id)
            .filter(|s| s.user_id == user_id)
            .cloned()
            .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "there is no such voice input (it may have timed out)"))
    }

    /// Takes a chunk of a live input and answers with the text so far. The read that follows from it runs in the
    /// background: the answer is the text as of the last read, and the next chunk's answer has the newer one.
    pub async fn push(self: &Arc<Self>, user_id: i64, id: &str, audio: Vec<f32>) -> Result<String, ErrorService> {
        let session = self.session(user_id, id)?;
        let job = session.push(&audio).map_err(|()| ErrorService::new(StatusCode::PAYLOAD_TOO_LARGE, "a voice input may be five minutes long at most"))?;
        if let Some(job) = job {
            let service = self.clone();
            let session = session.clone();
            tokio::spawn(async move { service.run_job(&session, job).await });
        }
        Ok(session.text())
    }

    async fn run_job(self: &Arc<Self>, session: &Arc<Session>, job: Job) {
        match self.read(session.model, session.language.clone(), job.audio().to_vec()).await {
            Ok(text) => {
                session.apply(&job, &text);
            }
            // The session goes on with the text it has: the next chunk tries again
            Err(_) => session.failed(),
        }
    }

    /// Ends a live input: reads what is left and answers with the whole text
    pub async fn finish(self: &Arc<Self>, user_id: i64, id: &str) -> Result<String, ErrorService> {
        let session = self.session(user_id, id)?;
        let started = std::time::Instant::now();
        while session.is_reading() && started.elapsed() < FINISH_WAIT {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let (rest, committed) = session.rest();
        let result = match rest {
            Some(audio) => {
                let tail = self.read(session.model, session.language.clone(), audio).await;
                tail.map(|tail| match (committed.is_empty(), tail.is_empty()) {
                    (true, _) => tail,
                    (_, true) => committed.clone(),
                    _ => format!("{committed} {tail}"),
                })
            }
            None => Ok(committed),
        };
        self.sessions().remove(id);
        // A read of audio that was quiet or ended in silence can come back empty after the live text had words: the
        // user saw those, so they stay instead of the box being emptied
        match result {
            Ok(text) if text.is_empty() => Ok(session.kept()),
            other => other,
        }
    }

    /// Drops a live input without reading the rest
    pub fn cancel(&self, user_id: i64, id: &str) {
        let mut sessions = self.sessions();
        if sessions.get(id).is_some_and(|s| s.user_id == user_id) {
            sessions.remove(id);
        }
    }
}

/// A position in a recording as `h:mm:ss`, or `m:ss` under an hour
fn clock(samples: usize) -> String {
    let secs = samples / SAMPLE_RATE;
    match secs / 3600 {
        0 => format!("{}:{:02}", secs / 60, secs % 60),
        hours => format!("{hours}:{:02}:{:02}", secs % 3600 / 60, secs % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_is_scaled_to_floats() {
        let bytes = [0x00, 0x00, 0xff, 0x7f, 0x00, 0x80];
        let floats = pcm16_to_f32(&bytes);
        assert_eq!(floats.len(), 3);
        assert_eq!(floats[0], 0.0);
        assert!((floats[1] - 0.99997).abs() < 1e-4);
        assert_eq!(floats[2], -1.0);
        // A trailing odd byte is not a sample
        assert_eq!(pcm16_to_f32(&[1, 2, 3]).len(), 1);
    }
}
