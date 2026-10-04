//! Runs llama.cpp's `llama-server` as a child process of the backend and decides which launch
//! profile it is running.
//!
//! One server, one loaded model. Callers ask for the profile they need through `acquire`; if the
//! server already runs it the call is free, if it runs another one it is restarted — unless somebody
//! is in the middle of a turn on that other one, in which case the caller gets a 423 naming who has it.
//! Reloading throws away the prompt cache, which is what a long agent turn lives on, so the holder
//! is never interrupted: the lease is the `CallGuard` a turn keeps for as long as it runs.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::{watch, Mutex as AsyncMutex};
use utoipa::ToSchema;

use super::event_bus::{EventBus, ModelStateKind, ServerEvent};
use super::error::ErrorService;
use super::gguf::read_gguf_info;
use super::launch_store::LaunchProfile;
use super::llm::LaunchRequest;
use crate::config::LlamaCppConfig;

/// How many log lines of the server are kept for the Models page.
const LOG_CAPACITY: usize = 2000;
/// How long a stopping server gets to exit on SIGTERM before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(10);
/// How long a request needing another profile waits for the turns on the loaded one before it is
/// refused with a 423.
const QUEUE_LIMIT: Duration = Duration::from_secs(15 * 60);
/// How often the idle watcher looks.
const IDLE_CHECK: Duration = Duration::from_secs(30);

pub struct LlamaRuntime {
    config: LlamaCppConfig,
    /// The settings the owner can change while the backend runs (see `set_tuning`)
    tuning: Mutex<Tuning>,
    events: Arc<EventBus>,
    http: reqwest::Client,
    inner: Arc<Mutex<Inner>>,
    /// Held for a whole load or switch, so two callers wanting different profiles queue instead of
    /// starting two servers on one port. Never held while only *using* the loaded model.
    switch: AsyncMutex<()>,
    logs: Arc<Mutex<LogBook>>,
    /// What a call that names no launch runs on when nothing is loaded: the owner's default.
    fallback: Mutex<Option<LaunchRequest>>,
    /// Whether a model file carries an MTP head, by path and the file's size and modification time:
    /// reading a GGUF header is file I/O, so it is done once per file, off the async runtime.
    mtp_heads: Mutex<HashMap<PathBuf, (std::time::SystemTime, u64, bool)>>,
    /// Run before a model server is started: frees what another provider holds on the GPU.
    before_start: Mutex<Option<BeforeStart>>,
    /// Woken whenever a claim is released, for the requests queued behind it
    released: tokio::sync::Notify,
}

type BeforeStart = Arc<dyn Fn() -> futures_util::future::BoxFuture<'static, ()> + Send + Sync>;

#[derive(Default)]
struct Inner {
    state: State,
    loaded: Option<Loaded>,
    /// Turns running right now, by launch profile id
    holds: HashMap<i64, Vec<String>>,
    /// Requests waiting in `ensure_loaded` for the loaded profile to be free of its turns
    waiting: usize,
    last_activity: Option<Instant>,
}

#[derive(Default, Clone)]
enum State {
    #[default]
    Stopped,
    Starting,
    Ready,
    Failed(String),
}

struct Loaded {
    request: LaunchRequest,
    args: Vec<String>,
    pid: u32,
    started: Instant,
    /// Tells the monitor task to stop the process
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    /// Flips to true when the process is gone
    exited: watch::Receiver<bool>,
}

/// What the owner can change about the server without restarting the backend.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Tuning {
    /// Stop the server after this many idle minutes, freeing the GPU; 0 never stops it
    pub idle_unload_minutes: u64,
    /// Load the default model when the backend starts (read at startup; changing it applies from the next start)
    pub autostart: bool,
    /// How long to wait for a model to finish loading, in seconds
    pub load_timeout_secs: u64,
}

impl Tuning {
    pub fn of(config: &LlamaCppConfig) -> Self {
        Self { idle_unload_minutes: config.idle_unload_minutes, autostart: config.autostart, load_timeout_secs: config.load_timeout_secs }
    }

    /// Refuses what would make no sense: a load timeout too short for any model, an idle time of years.
    pub fn validate(&self) -> Result<(), ErrorService> {
        if self.idle_unload_minutes > 10_080 {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, "the idle time can be at most a week (10080 minutes); 0 never unloads"));
        }
        if !(10..=3600).contains(&self.load_timeout_secs) {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, "the load timeout must be between 10 and 3600 seconds"));
        }
        Ok(())
    }
}

/// A running turn's claim on the server. Dropping it ends the claim and counts as activity for the
/// idle timer.
pub struct CallGuard {
    release: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl CallGuard {
    pub fn none() -> Self {
        Self { release: None }
    }
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

#[derive(Default)]
struct LogBook {
    lines: VecDeque<String>,
    /// How many lines have ever been pushed; the index of the next one
    total: usize,
    facts: LoadFacts,
}

/// What the server's own startup log says about where the model went.
#[derive(Clone, Default, Serialize, ToSchema)]
pub struct LoadFacts {
    pub layers_offloaded: Option<u32>,
    pub layers_total: Option<u32>,
    /// Memory buffers the server reported, like `ROCm0 model buffer size`
    pub buffers: Vec<MemoryBuffer>,
    /// The context each slot got, which with `--fit` is the server's own choice
    pub context_per_slot: Option<u64>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct MemoryBuffer {
    pub name: String,
    pub mib: f64,
}

#[derive(Serialize, ToSchema)]
pub struct RuntimeStatus {
    /// `stopped`, `starting`, `ready`, `failed`, `not_installed` or `external`
    pub state: String,
    pub profile_id: Option<i64>,
    pub model: Option<String>,
    /// Why it failed, or what is missing
    pub detail: Option<String>,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    /// Who has a turn running on the loaded profile right now
    pub holders: Vec<String>,
    /// How many requests are waiting for the model to be free of those turns
    pub queued: usize,
    pub facts: LoadFacts,
    /// Whether the loaded model is really on the GPU; filled in by the route that serves the status
    pub placement: Option<crate::facade::placement::Placement>,
    pub binary: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug)]
pub enum RuntimeErrors {
    NotInstalled(PathBuf),
    MissingModel(PathBuf),
    Busy { holders: String },
    NothingToRun,
    External,
    LoadFailed(String),
    Spawn(String),
}

impl std::fmt::Display for RuntimeErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled(path) => write!(f, "llama.cpp isn't installed (looked for {}); run the setup step to download it", path.display()),
            Self::MissingModel(path) => write!(f, "the model file {} doesn't exist", path.display()),
            Self::Busy { holders } => write!(f, "the model is in use by {holders}: wait for their turn to finish, then try again"),
            Self::NothingToRun => write!(f, "no model has been loaded yet: pick one on the Models page"),
            Self::External => write!(f, "the model server can't be restarted from here: it is an external one (`llama_cpp.external_url`)"),
            Self::LoadFailed(reason) => write!(f, "the model failed to load: {reason}"),
            Self::Spawn(reason) => write!(f, "could not start llama-server: {reason}"),
        }
    }
}

impl From<RuntimeErrors> for ErrorService {
    fn from(err: RuntimeErrors) -> Self {
        let status = match &err {
            RuntimeErrors::Busy { .. } => StatusCode::LOCKED,
            RuntimeErrors::NotInstalled(_) | RuntimeErrors::NothingToRun | RuntimeErrors::External => StatusCode::CONFLICT,
            RuntimeErrors::MissingModel(_) => StatusCode::NOT_FOUND,
            RuntimeErrors::LoadFailed(_) | RuntimeErrors::Spawn(_) => StatusCode::BAD_GATEWAY,
        };
        ErrorService::new(status, err.to_string())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock can't leave this state half-written in a way that matters more
    // than refusing to work at all would.
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl LlamaRuntime {
    pub fn new(config: LlamaCppConfig, events: Arc<EventBus>) -> Arc<Self> {
        let runtime = Arc::new(Self {
            tuning: Mutex::new(Tuning::of(&config)),
            config,
            events,
            http: reqwest::Client::new(),
            inner: Arc::new(Mutex::new(Inner::default())),
            switch: AsyncMutex::new(()),
            logs: Arc::new(Mutex::new(LogBook::default())),
            fallback: Mutex::new(None),
            mtp_heads: Mutex::new(HashMap::new()),
            before_start: Mutex::new(None),
            released: tokio::sync::Notify::new(),
        });
        runtime.clone().spawn_idle_watcher();
        runtime
    }

    pub fn tuning(&self) -> Tuning {
        lock(&self.tuning).clone()
    }

    /// Applies new settings at once: the idle watcher and the next load read them afresh.
    pub fn set_tuning(&self, tuning: Tuning) {
        *lock(&self.tuning) = tuning;
    }

    /// Registers what runs before every start of the server (see `before_start`).
    pub fn set_before_start(&self, hook: impl Fn() -> futures_util::future::BoxFuture<'static, ()> + Send + Sync + 'static) {
        *lock(&self.before_start) = Some(Arc::new(hook));
    }

    /// Stops the server so another provider can have the GPU, unless somebody is in the middle of a
    /// turn on it (then it stays, and the other provider makes do). Best effort.
    pub async fn release_gpu(self: &Arc<Self>) {
        if self.is_external() {
            return;
        }
        let running = matches!(lock(&self.inner).state, State::Ready | State::Starting);
        if running && self.stop().await.is_ok() {
            tracing::info!("stopped llama-server to free the GPU for Ollama");
        }
    }

    pub fn is_external(&self) -> bool {
        self.config.external_url.is_some()
    }

    pub fn binary_path(&self) -> PathBuf {
        let dir = self.config.resolved_dir();
        // The installer records where the binary is: inside the unpacked release, or wherever the
        // user's own build lives.
        let recorded = std::fs::read_to_string(dir.join("install.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|meta| meta["binary"].as_str().map(PathBuf::from));
        match recorded {
            Some(path) if path.is_absolute() => path,
            Some(path) => dir.join(path),
            None => dir.join(if cfg!(windows) { "llama-server.exe" } else { "llama-server" }),
        }
    }

    /// The owner's default launch: what a call that names none runs when nothing is loaded, and what
    /// is loaded right away when `autostart` is on.
    pub fn remember_default(self: &Arc<Self>, request: LaunchRequest) {
        lock(&self.fallback).get_or_insert_with(|| request.clone());
        if self.tuning().autostart && !self.is_external() {
            let runtime = self.clone();
            tokio::spawn(async move {
                if let Err(e) = runtime.load(&request).await {
                    tracing::warn!("autostart of the model server failed: {}", e.message.unwrap_or_default());
                }
            });
        }
    }

    /// A user changed their default model: calls that name no launch fall back to it from now on.
    pub fn set_default(&self, request: LaunchRequest) {
        *lock(&self.fallback) = Some(request);
    }

    /// The default model is gone (removed) and nothing replaces it yet: a call that names no launch
    /// runs on whatever is loaded, or fails until a model is chosen.
    pub fn clear_default(&self) {
        *lock(&self.fallback) = None;
    }

    /// Makes sure the server runs `request` (or whatever is loaded, for `None`) and claims it for the
    /// caller until the guard is dropped.
    pub async fn acquire(self: &Arc<Self>, request: Option<&LaunchRequest>) -> Result<CallGuard, ErrorService> {
        if self.is_external() {
            return Ok(CallGuard::none());
        }

        if let Some(request) = request {
            // The next call that names no launch of its own runs on what was last asked for.
            *lock(&self.fallback) = Some(request.clone());
        }
        let wanted = match request {
            Some(request) => request.clone(),
            None => self.running_request().or_else(|| lock(&self.fallback).clone()).ok_or(RuntimeErrors::NothingToRun)?,
        };
        // A call with no launch of its own is happy with whatever is loaded, even if the owner's
        // default is something else.
        let any_loaded = request.is_none() && self.running_request().is_some();
        let args = self.launch_args(&wanted).await;

        if let Some(guard) = self.try_claim(&wanted, &args, any_loaded) {
            return Ok(guard);
        }

        let _switching = self.switch.lock().await;
        // Whoever held the switch lock before us may have loaded exactly this.
        if let Some(guard) = self.try_claim(&wanted, &args, any_loaded) {
            return Ok(guard);
        }
        self.ensure_loaded(&wanted, args.clone()).await?;
        self.try_claim(&wanted, &args, true).ok_or_else(|| RuntimeErrors::LoadFailed("the server stopped right after loading".into()).into())
    }

    /// The command line for a launch. The model file's header is read (once per file) to see whether
    /// it has an MTP head, on the blocking pool.
    async fn launch_args(&self, request: &LaunchRequest) -> Vec<String> {
        let path = request.model_file.clone();
        let stamp = std::fs::metadata(&path).ok().map(|m| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len()));
        let cached = stamp.and_then(|(modified, len)| {
            lock(&self.mtp_heads).get(&path).filter(|(m, l, _)| (*m, *l) == (modified, len)).map(|(_, _, has)| *has)
        });
        let has_mtp = match (cached, stamp) {
            (Some(has), _) => has,
            (None, stamp) => {
                let read = path.clone();
                let has = tokio::task::spawn_blocking(move || read_gguf_info(&read).map(|info| info.has_mtp()).unwrap_or(false))
                    .await
                    .unwrap_or(false);
                if let Some((modified, len)) = stamp {
                    lock(&self.mtp_heads).insert(path, (modified, len, has));
                }
                has
            }
        };
        build_args(&self.config, request, has_mtp)
    }

    /// The launch that is loaded and ready, with the server's process id
    pub fn loaded_launch(&self) -> Option<(LaunchRequest, u32)> {
        let inner = lock(&self.inner);
        let loaded = inner.loaded.as_ref()?;
        matches!(inner.state, State::Ready).then(|| (loaded.request.clone(), loaded.pid))
    }

    fn running_request(&self) -> Option<LaunchRequest> {
        let inner = lock(&self.inner);
        match inner.state {
            State::Ready | State::Starting => inner.loaded.as_ref().map(|l| l.request.clone()),
            _ => None,
        }
    }

    /// The claim, if the server is ready on what's wanted.
    fn try_claim(self: &Arc<Self>, wanted: &LaunchRequest, args: &[String], any_loaded: bool) -> Option<CallGuard> {
        let mut inner = lock(&self.inner);
        let loaded = inner.loaded.as_ref()?;
        if !matches!(inner.state, State::Ready) || *loaded.exited.borrow() {
            return None;
        }
        // A request is queued for a switch: newcomers line up behind it, so a steady stream of calls on
        // the loaded profile can't keep it waiting for ever. Whoever already holds a claim (the next
        // call of a turn in progress) goes on, or the turn would wait for the very switch waiting for it.
        let loaded_id = loaded.request.profile.id;
        if inner.waiting > 0 && !inner.holds.get(&loaded_id).is_some_and(|h| h.contains(&wanted.holder)) {
            return None;
        }
        let loaded = inner.loaded.as_ref()?;
        let same = if any_loaded {
            true
        } else {
            loaded.request.profile.id == wanted.profile.id && loaded.args == args
        };
        if !same {
            return None;
        }
        let profile_id = loaded.request.profile.id;
        inner.holds.entry(profile_id).or_default().push(wanted.holder.clone());
        inner.last_activity = Some(Instant::now());
        drop(inner);

        let runtime = self.clone();
        let holder = wanted.holder.clone();
        Some(CallGuard {
            release: Some(Box::new(move || {
                let mut inner = lock(&runtime.inner);
                if let Some(holders) = inner.holds.get_mut(&profile_id) {
                    if let Some(at) = holders.iter().position(|h| *h == holder) {
                        holders.remove(at);
                    }
                    if holders.is_empty() {
                        inner.holds.remove(&profile_id);
                    }
                }
                inner.last_activity = Some(Instant::now());
                drop(inner);
                runtime.released.notify_waiters();
            })),
        })
    }

    /// Who has a claim on the loaded server right now, by name.
    fn busy_holders(&self) -> Vec<String> {
        let inner = lock(&self.inner);
        let loaded_alive = inner.loaded.as_ref().is_some_and(|l| !*l.exited.borrow());
        let mut names: Vec<String> = if loaded_alive { inner.holds.values().flatten().cloned().collect() } else { Vec::new() };
        names.sort();
        names.dedup();
        names
    }

    /// Waits until nobody has a claim on the loaded server, then stops it and starts `wanted`. Called
    /// with the switch lock held, so requests wanting a different profile are served in the order
    /// they came, and what is running is never cut off: a turn, a greeting or a summary finishes
    /// first. Refused with a 423 only after `QUEUE_LIMIT`.
    async fn ensure_loaded(self: &Arc<Self>, wanted: &LaunchRequest, args: Vec<String>) -> Result<(), ErrorService> {
        let queued_at = Instant::now();
        let mut announced = false;
        let outcome = loop {
            // Created before looking, so a release between the look and the wait isn't missed
            let released = self.released.notified();
            let busy = self.busy_holders();
            if busy.is_empty() {
                break Ok(());
            }
            let Some(left) = QUEUE_LIMIT.checked_sub(queued_at.elapsed()) else {
                break Err(RuntimeErrors::Busy { holders: busy.join(", ") });
            };
            if !announced {
                announced = true;
                lock(&self.inner).waiting += 1;
                self.publish(ModelStateKind::Queued, Some(wanted), Some(format!("waiting for {}", busy.join(", "))));
            }
            let _ = tokio::time::timeout(left, released).await;
        };
        if announced {
            lock(&self.inner).waiting -= 1;
            self.released.notify_waiters();
        }
        outcome?;

        self.stop_inner().await;
        self.start(wanted, args).await.map_err(Into::into)
    }

    /// Loads `request` on request of the Models page, like `acquire` but without claiming it.
    pub async fn load(self: &Arc<Self>, request: &LaunchRequest) -> Result<(), ErrorService> {
        if self.is_external() {
            return Err(RuntimeErrors::External.into());
        }
        self.acquire(Some(request)).await.map(drop)
    }

    /// Stops the server. Refused while somebody's turn is running on it.
    pub async fn stop(self: &Arc<Self>) -> Result<(), ErrorService> {
        if self.is_external() {
            return Err(RuntimeErrors::External.into());
        }
        let _switching = self.switch.lock().await;
        let busy: Vec<String> = lock(&self.inner).holds.values().flatten().cloned().collect();
        if !busy.is_empty() {
            return Err(RuntimeErrors::Busy { holders: busy.join(", ") }.into());
        }
        self.stop_inner().await;
        Ok(())
    }

    /// Stops the process without asking anybody; for the backend's own shutdown.
    pub async fn shutdown(self: &Arc<Self>) {
        let _switching = self.switch.lock().await;
        self.stop_inner().await;
    }

    async fn stop_inner(&self) {
        let (stop, mut exited, request) = {
            let mut inner = lock(&self.inner);
            let Some(loaded) = inner.loaded.as_mut() else { return };
            (loaded.stop.take(), loaded.exited.clone(), loaded.request.clone())
        };
        if let Some(stop) = stop {
            let _ = stop.send(());
        }
        // The monitor sends the signal and escalates; here we only wait for it to report back.
        let _ = tokio::time::timeout(STOP_GRACE + Duration::from_secs(5), exited.wait_for(|gone| *gone)).await;
        {
            let mut inner = lock(&self.inner);
            inner.loaded = None;
            inner.state = State::Stopped;
        }
        self.publish(ModelStateKind::Stopped, Some(&request), None);
    }

    async fn start(self: &Arc<Self>, request: &LaunchRequest, args: Vec<String>) -> Result<(), RuntimeErrors> {
        let binary = self.binary_path();
        if !binary.is_file() {
            return Err(RuntimeErrors::NotInstalled(binary));
        }
        if !request.model_file.is_file() {
            return Err(RuntimeErrors::MissingModel(request.model_file.clone()));
        }

        {
            let mut book = lock(&self.logs);
            book.lines.clear();
            book.total = 0;
            book.facts = LoadFacts::default();
        }
        let hook = lock(&self.before_start).clone();
        if let Some(hook) = hook {
            hook().await;
        }
        tracing::info!("starting llama-server: {} {}", binary.display(), args.join(" "));

        let mut command = Command::new(&binary);
        command
            .args(&args)
            .current_dir(binary.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // If the backend dies without a chance to stop it, the model must not stay in VRAM.
            .kill_on_drop(true);
        if let Some(dir) = binary.parent() {
            // The release ships its libraries next to the binary.
            let key = if cfg!(windows) { "PATH" } else { "LD_LIBRARY_PATH" };
            let existing = std::env::var_os(key).unwrap_or_default();
            let mut paths = vec![dir.to_path_buf()];
            paths.extend(std::env::split_paths(&existing));
            if let Ok(joined) = std::env::join_paths(paths) {
                command.env(key, joined);
            }
        }
        let mut child = command.spawn().map_err(|e| RuntimeErrors::Spawn(e.to_string()))?;
        let pid = child.id().unwrap_or_default();
        if let Some(out) = child.stdout.take() {
            self.clone().pump_logs(out);
        }
        if let Some(err) = child.stderr.take() {
            self.clone().pump_logs(err);
        }

        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let (exited_tx, exited_rx) = watch::channel(false);
        {
            let mut inner = lock(&self.inner);
            inner.loaded = Some(Loaded {
                request: request.clone(),
                args,
                pid,
                started: Instant::now(),
                stop: Some(stop_tx),
                exited: exited_rx.clone(),
            });
            inner.state = State::Starting;
        }
        self.publish(ModelStateKind::Loading, Some(request), None);
        self.clone().monitor(child, pid, stop_rx, exited_tx);

        match self.wait_ready(exited_rx).await {
            Ok(()) => {
                lock(&self.inner).state = State::Ready;
                lock(&self.inner).last_activity = Some(Instant::now());
                self.publish(ModelStateKind::Ready, Some(request), None);
                Ok(())
            }
            Err(reason) => {
                let (stopping, exited) = {
                    let mut inner = lock(&self.inner);
                    match inner.loaded.as_mut() {
                        Some(loaded) => (loaded.stop.take(), Some(loaded.exited.clone())),
                        None => (None, None),
                    }
                };
                if let Some(stop) = stopping {
                    let _ = stop.send(());
                }
                // The next load must not start while this process still holds the port
                if let Some(mut exited) = exited {
                    let _ = tokio::time::timeout(STOP_GRACE + Duration::from_secs(5), exited.wait_for(|gone| *gone)).await;
                }
                // Said before the server's own words: it is what the owner can act on
                let reason = match memory_hint(&reason, &request.model_file) {
                    Some(hint) => format!("{hint} The server said: {reason}"),
                    None => reason,
                };
                {
                    let mut inner = lock(&self.inner);
                    inner.loaded = None;
                    inner.state = State::Failed(reason.clone());
                }
                self.publish(ModelStateKind::Failed, Some(request), Some(reason.clone()));
                Err(RuntimeErrors::LoadFailed(reason))
            }
        }
    }

    /// Owns the child: waits for it to exit on its own, or stops it when told.
    fn monitor(
        self: Arc<Self>,
        mut child: tokio::process::Child,
        pid: u32,
        stop: tokio::sync::oneshot::Receiver<()>,
        exited: watch::Sender<bool>,
    ) {
        tokio::spawn(async move {
            let mut asked = false;
            let status = tokio::select! {
                status = child.wait() => status,
                _ = stop => {
                    asked = true;
                    terminate(pid);
                    match tokio::time::timeout(STOP_GRACE, child.wait()).await {
                        Ok(status) => status,
                        Err(_) => {
                            let _ = child.kill().await;
                            child.wait().await
                        }
                    }
                }
            };
            let _ = exited.send(true);

            if asked {
                return;
            }
            // It went away on its own: a crash, or the user killed it.
            let (request, was_starting) = {
                let mut inner = lock(&self.inner);
                if inner.loaded.as_ref().map(|l| l.pid) != Some(pid) {
                    return;
                }
                let request = inner.loaded.take().map(|l| l.request);
                let was_starting = matches!(inner.state, State::Starting);
                inner.state = State::Stopped;
                (request, was_starting)
            };
            if was_starting {
                // `wait_ready` reports it with the log tail; nothing to add.
                return;
            }
            let reason = format!("llama-server exited unexpectedly ({})", status.map(|s| s.to_string()).unwrap_or_default());
            tracing::warn!("{reason}");
            lock(&self.inner).state = State::Failed(reason.clone());
            self.publish(ModelStateKind::Failed, request.as_ref(), Some(reason));
        });
    }

    async fn wait_ready(&self, mut exited: watch::Receiver<bool>) -> Result<(), String> {
        let load_timeout_secs = self.tuning().load_timeout_secs;
        let deadline = Instant::now() + Duration::from_secs(load_timeout_secs);
        let health = format!("{}/health", self.config.base_url());
        loop {
            if *exited.borrow_and_update() {
                return Err(self.failure_reason("llama-server exited while loading the model"));
            }
            if Instant::now() > deadline {
                return Err(self.failure_reason(&format!("the model didn't finish loading in {load_timeout_secs} seconds")));
            }
            let ok = self.http.get(&health).timeout(Duration::from_secs(2)).send().await.is_ok_and(|r| r.status().is_success());
            if ok {
                return Ok(());
            }
            let _ = tokio::time::timeout(Duration::from_millis(300), exited.changed()).await;
        }
    }

    /// `headline` plus the server's own last words, which is where the real reason is (out of
    /// memory, a missing library, an unsupported architecture).
    fn failure_reason(&self, headline: &str) -> String {
        let book = lock(&self.logs);
        let tail: Vec<&str> = book.lines.iter().rev().map(|l| l.as_str()).filter(|l| !l.trim().is_empty()).take(6).collect();
        if tail.is_empty() {
            headline.to_string()
        } else {
            format!("{headline}: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | "))
        }
    }

    fn pump_logs<R: AsyncRead + Unpin + Send + 'static>(self: Arc<Self>, source: R) {
        tokio::spawn(async move {
            let mut lines = BufReader::new(source).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut book = lock(&self.logs);
                parse_fact(&mut book.facts, &line);
                if book.lines.len() == LOG_CAPACITY {
                    book.lines.pop_front();
                }
                book.lines.push_back(line);
                book.total += 1;
            }
        });
    }

    fn publish(&self, state: ModelStateKind, request: Option<&LaunchRequest>, detail: Option<String>) {
        self.events.publish(ServerEvent::ModelState {
            state,
            profile_id: request.map(|r| r.profile.id),
            model: request.and_then(|r| r.model_file.file_name()).map(|n| n.to_string_lossy().into_owned()),
            detail,
        });
    }

    fn spawn_idle_watcher(self: Arc<Self>) {
        if self.is_external() {
            return;
        }
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(IDLE_CHECK).await;
                // Read afresh every round: the owner can change it, or turn it on, while the backend runs
                let minutes = self.tuning().idle_unload_minutes;
                if minutes == 0 {
                    continue;
                }
                let limit = Duration::from_secs(minutes * 60);
                let idle = {
                    let inner = lock(&self.inner);
                    matches!(inner.state, State::Ready)
                        && inner.holds.is_empty()
                        && inner.last_activity.is_some_and(|at| at.elapsed() >= limit)
                };
                if idle {
                    tracing::info!("unloading the model after {minutes} idle minutes");
                    // Somebody claiming it between the check and the stop is turned away by `stop`.
                    let _ = self.stop().await;
                }
            }
        });
    }

    pub fn status(&self) -> RuntimeStatus {
        let binary = self.binary_path();
        let facts = lock(&self.logs).facts.clone();
        if self.is_external() {
            return RuntimeStatus {
                state: "external".into(),
                profile_id: None,
                model: None,
                detail: self.config.external_url.clone(),
                pid: None,
                uptime_secs: None,
                holders: Vec::new(),
                queued: 0,
                facts,
                placement: None,
                binary: None,
                version: None,
            };
        }
        let inner = lock(&self.inner);
        let (state, detail) = match &inner.state {
            State::Stopped if !binary.is_file() => ("not_installed", Some(format!("{} doesn't exist", binary.display()))),
            State::Stopped => ("stopped", None),
            State::Starting => ("starting", None),
            State::Ready => ("ready", None),
            State::Failed(reason) => ("failed", Some(reason.clone())),
        };
        let loaded = inner.loaded.as_ref();
        let mut holders: Vec<String> = loaded.map(|l| inner.holds.get(&l.request.profile.id).cloned().unwrap_or_default()).unwrap_or_default();
        holders.sort();
        holders.dedup();
        RuntimeStatus {
            state: state.into(),
            profile_id: loaded.map(|l| l.request.profile.id),
            model: loaded.and_then(|l| l.request.model_file.file_name()).map(|n| n.to_string_lossy().into_owned()),
            detail,
            pid: loaded.map(|l| l.pid),
            uptime_secs: loaded.map(|l| l.started.elapsed().as_secs()),
            holders,
            queued: inner.waiting,
            facts,
            placement: None,
            binary: binary.is_file().then(|| binary.display().to_string()),
            version: None,
        }
    }

    /// The last `limit` log lines, with the index of the first one returned (`since` asks for lines
    /// from that index on, so a poller doesn't refetch what it has).
    pub fn logs(&self, since: Option<usize>, limit: usize) -> (usize, Vec<String>) {
        let book = lock(&self.logs);
        let first_kept = book.total - book.lines.len();
        // A position from before the log was cleared (a new load) starts over
        let since = since.filter(|s| *s <= book.total);
        let start = since.unwrap_or(0).max(first_kept).max(book.total.saturating_sub(limit));
        (start, book.lines.iter().skip(start - first_kept).cloned().collect())
    }

    /// How many lines the log has had so far, for `logs_since`.
    pub fn log_position(&self) -> usize {
        lock(&self.logs).total
    }

    /// The `llama-server --list-devices` output: the authority on what GPUs it can use.
    pub async fn list_devices(&self) -> Result<String, RuntimeErrors> {
        let binary = self.binary_path();
        if !binary.is_file() {
            return Err(RuntimeErrors::NotInstalled(binary));
        }
        let output = Command::new(&binary)
            .arg("--list-devices")
            .current_dir(binary.parent().unwrap_or(Path::new(".")))
            .env("LD_LIBRARY_PATH", binary.parent().unwrap_or(Path::new(".")))
            .output()
            .await
            .map_err(|e| RuntimeErrors::Spawn(e.to_string()))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok(text)
    }
}

/// Asks the process to exit, the way a terminal's Ctrl-C does; Windows has no such signal, so there
/// it is killed outright.
fn terminate(pid: u32) {
    use sysinfo::{Pid, ProcessesToUpdate, Signal, System};
    let mut system = System::new();
    let pid = Pid::from_u32(pid);
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    if let Some(process) = system.process(pid) {
        if process.kill_with(Signal::Term).is_none() {
            process.kill();
        }
    }
}

/// The `llama-server` command line for a launch. Pure, so what a profile turns into can be tested
/// and shown to the user.
/// Whether a failed load's log says the memory ran out, rather than something else going wrong.
fn is_memory_failure(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    ["out of memory", "unable to allocate", "failed to allocate", "cannot allocate"].iter().any(|s| reason.contains(s))
}

/// What to tell the owner when the model didn't fit: its size against what the card has, and what to change.
fn memory_hint(reason: &str, model_file: &Path) -> Option<String> {
    if !is_memory_failure(reason) {
        return None;
    }
    let model_mib = std::fs::metadata(model_file).ok().map(|m| m.len() / (1024 * 1024));
    Some(describe_memory_failure(model_mib, crate::services::gpu_memory::biggest_card()))
}

fn describe_memory_failure(model_mib: Option<u64>, card: Option<(u64, u64)>) -> String {
    // Decimal gigabytes, like every other size the app shows
    let gb = |mib: u64| format!("{:.1} GB", (mib * 1024 * 1024) as f64 / 1e9);
    let sizes = match (model_mib, card) {
        (Some(model), Some((total, free))) => format!("The model file is {} and the card has {} free of {}.", gb(model), gb(free), gb(total)),
        (Some(model), None) => format!("The model file is {}.", gb(model)),
        _ => String::new(),
    };
    format!(
        "The model doesn't fit in the GPU's memory. {sizes} Lower \"Layers on the GPU\" in its launch profile, use a smaller quantization, or shrink the context or the KV cache.",
    )
    .replace("  ", " ")
}

pub fn build_args(config: &LlamaCppConfig, request: &LaunchRequest, has_mtp: bool) -> Vec<String> {
    let profile: &LaunchProfile = &request.profile;
    let mut args: Vec<String> = Vec::new();
    // Flags that take no value
    let mut bare: Vec<String> = Vec::new();
    let mut push = |flag: &str, value: String| {
        args.push(flag.to_string());
        args.push(value);
    };

    push("-m", request.model_file.display().to_string());
    push("--host", "127.0.0.1".into());
    push("--port", config.port.to_string());
    push("-ngl", profile.gpu_layers.to_string());
    push("--flash-attn", if profile.flash_attn { "on" } else { "off" }.into());
    push("--cache-type-k", profile.cache_type_k.clone());
    push("--cache-type-v", profile.cache_type_v.clone());
    match profile.context_length {
        Some(context) => push("-c", context.to_string()),
        // Sized to free memory rather than a number somebody had to guess.
        None => push("--fit", "on".into()),
    }
    // The agent runs one conversation at a time, and a single slot keeps its whole prompt cached.
    push("-np", "1".into());
    if let Some(mmproj) = &profile.mmproj_file {
        push("--mmproj", request.model_root.join(mmproj).display().to_string());
        if !profile.mmproj_gpu {
            // The projector's weights and compute buffers stay in RAM: less VRAM, slower image reading
            bare.push("--no-mmproj-offload".to_string());
        }
    }
    // The draft head lives in the model file; a file without one can't use it, and asking anyway
    // makes the server refuse to start.
    if profile.mtp && has_mtp {
        push("--spec-type", "draft-mtp,ngram-mod".into());
        push("--spec-draft-n-max", profile.spec_draft_n_max.to_string());
        push("--spec-ngram-mod-n-match", profile.ngram_match.to_string());
        push("--spec-ngram-mod-n-min", profile.ngram_min.to_string());
        push("--spec-ngram-mod-n-max", profile.ngram_max.to_string());
    }
    args.extend(["--jinja".to_string(), "--metrics".to_string()]);
    args.extend(bare);
    args.extend(profile.extra_args.split_whitespace().map(str::to_string));
    args
}

/// Picks what the startup log says about the model's placement.
fn parse_fact(facts: &mut LoadFacts, line: &str) {
    if let Some(rest) = line.split("offloaded ").nth(1) {
        if let Some((counts, _)) = rest.split_once(" layers") {
            if let Some((done, total)) = counts.split_once('/') {
                facts.layers_offloaded = done.trim().parse().ok();
                facts.layers_total = total.trim().parse().ok();
            }
        }
    }
    if let Some((before, after)) = line.split_once(" model buffer size =") {
        let name = before.rsplit(' ').next().unwrap_or_default().trim_end_matches(':').to_string();
        let mib = after.trim().split_whitespace().next().and_then(|n| n.parse::<f64>().ok());
        if let (false, Some(mib)) = (name.is_empty(), mib) {
            facts.buffers.retain(|b| b.name != name);
            facts.buffers.push(MemoryBuffer { name, mib });
        }
    }
    if let Some(rest) = line.split("n_ctx_slot = ").nth(1) {
        facts.context_per_slot = rest.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse().ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_load_is_told_apart_by_what_the_server_says() {
        assert!(is_memory_failure("E ggml_backend_cuda_buffer_type_alloc_buffer: cudaMalloc failed: out of memory"));
        assert!(is_memory_failure("E llama_model_load: error loading model: unable to allocate ROCm0 buffer"));
        assert!(!is_memory_failure("error while loading shared libraries: libhipblas.so.2: cannot open shared object file"));
    }

    #[test]
    fn the_hint_gives_the_sizes_when_they_are_known() {
        let hint = describe_memory_failure(Some(22_320), Some((16_368, 14_300)));
        assert!(hint.contains("23.4 GB") && hint.contains("15.0 GB free of 17.2 GB"), "{hint}");
        assert!(hint.contains("Layers on the GPU"));
        let without_card = describe_memory_failure(Some(22_320), None);
        assert!(without_card.contains("The model file is 23.4 GB.") && !without_card.contains("free of"), "{without_card}");
        assert!(!describe_memory_failure(None, None).contains("  "));
    }

    fn profile() -> LaunchProfile {
        LaunchProfile {
            id: 1,
            model_id: 1,
            name: "default".into(),
            mmproj_file: None,
            mmproj_gpu: true,
            context_length: Some(32768),
            cache_type_k: "q8_0".into(),
            cache_type_v: "q8_0".into(),
            flash_attn: true,
            gpu_layers: 99,
            mtp: true,
            spec_draft_n_max: 3,
            ngram_match: 24,
            ngram_min: 48,
            ngram_max: 64,
            extra_args: "--no-mmap  --threads 4".into(),
        }
    }

    fn request(profile: LaunchProfile) -> LaunchRequest {
        LaunchRequest { profile, model_file: "/models/sub/not-a-gguf.gguf".into(), model_root: "/models".into(), holder: "a".into() }
    }

    #[test]
    fn args_follow_the_profile() {
        let args = build_args(&LlamaCppConfig::default(), &request(profile()), false);
        let joined = args.join(" ");
        assert!(joined.starts_with("-m /models/sub/not-a-gguf.gguf --host 127.0.0.1 --port 18080 -ngl 99 --flash-attn on"));
        assert!(joined.contains("-c 32768"));
        assert!(joined.ends_with("--jinja --metrics --no-mmap --threads 4"));
        // No MTP head in the file, so no drafting flags even though the profile asks for it.
        assert!(!joined.contains("--spec-type"));
        let with_head = build_args(&LlamaCppConfig::default(), &request(profile()), true).join(" ");
        assert!(with_head.contains("--spec-type draft-mtp,ngram-mod --spec-draft-n-max 3"));
    }

    #[test]
    fn automatic_context_and_projector() {
        let mut p = profile();
        p.context_length = None;
        p.flash_attn = false;
        p.mmproj_file = Some("mmproj-f16.gguf".into());
        let joined = build_args(&LlamaCppConfig::default(), &request(p), false).join(" ");
        assert!(joined.contains("--fit on") && !joined.contains(" -c "));
        assert!(joined.contains("--flash-attn off"));
        // The projector is relative to the model folder, not to the model file's own subfolder
        assert!(joined.contains("--mmproj /models/mmproj-f16.gguf") && !joined.contains("--no-mmproj-offload"));
        let mut cpu = profile();
        cpu.mmproj_file = Some("mmproj-f16.gguf".into());
        cpu.mmproj_gpu = false;
        assert!(build_args(&LlamaCppConfig::default(), &request(cpu), false).join(" ").contains("--no-mmproj-offload"));
        // Without a projector there is nothing to place
        let mut none = profile();
        none.mmproj_gpu = false;
        assert!(!build_args(&LlamaCppConfig::default(), &request(none), false).join(" ").contains("--no-mmproj-offload"));
    }

    #[test]
    fn server_settings_are_range_checked() {
        let ok = Tuning { idle_unload_minutes: 5, autostart: false, load_timeout_secs: 300 };
        assert!(ok.validate().is_ok());
        assert!(Tuning { idle_unload_minutes: 0, ..ok.clone() }.validate().is_ok(), "0 means never");
        assert!(Tuning { idle_unload_minutes: 10_081, ..ok.clone() }.validate().is_err());
        assert!(Tuning { load_timeout_secs: 5, ..ok.clone() }.validate().is_err());
        assert!(Tuning { load_timeout_secs: 3601, ..ok }.validate().is_err());
    }

    #[test]
    fn startup_log_facts() {
        let mut facts = LoadFacts::default();
        for line in [
            "load_tensors: offloaded 41/41 layers to GPU",
            "load_tensors:   ROCm0 model buffer size = 13000.50 MiB",
            "load_tensors:   CPU_Mapped model buffer size =   500.00 MiB",
            "srv    load_model: initializing, n_slots = 1, n_ctx_slot = 32768, kv_unified = false",
        ] {
            parse_fact(&mut facts, line);
        }
        assert_eq!((facts.layers_offloaded, facts.layers_total), (Some(41), Some(41)));
        assert_eq!(facts.buffers.len(), 2);
        assert_eq!(facts.buffers[0].name, "ROCm0");
        assert_eq!(facts.context_per_slot, Some(32768));
    }
}
