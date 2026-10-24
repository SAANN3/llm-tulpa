//! Keeps one external plugin's process running while the plugin is switched on. A plugin that is off has no process: `run`
//! starts one when the plugin is switched on, `stop` ends it when it is switched off. In between, every call to the plugin goes
//! through here, so the rest of the backend holds a `Supervisor` and never a process: when the process ends without having been
//! asked to, a new one is started (after a growing pause), told the settings and that the plugin is on, and the calls that were
//! refused meanwhile work again. A plugin that keeps crashing is given up on and an event says so, so the registry can switch
//! it off.
//!
//! The tools of a plugin that is being restarted stay registered. Taking them out and putting them back would change the
//! tool list the model is sent, for a pause of a second or so, and a call in that time fails with a message that says the
//! plugin is restarting.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::{MutexGuard, mpsc};
use utoipa::ToSchema;

use super::db::PluginDb;
use super::manifest::Manifest;
use super::process::{LogRing, PluginProcess};
use super::rpc::{CallError, METHOD_NOT_FOUND};

/// The pause before the next start, by how many crashes there have been lately (the last value holds for more)
const BACKOFF: [Duration; 3] = [Duration::from_secs(1), Duration::from_secs(5), Duration::from_secs(30)];
/// This many crashes within `CRASH_WINDOW` and the plugin is given up on
const MAX_CRASHES: usize = 5;
const CRASH_WINDOW: Duration = Duration::from_secs(10 * 60);
/// A plugin that fails to answer this many calls in a row is taken to be stuck and is restarted
const MAX_TIMEOUTS_IN_A_ROW: u32 = 3;
/// How long the settings and the switch may take to be handed to a process that has just started
const REPLAY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// No process: the plugin is switched off, or was never switched on since the backend started
    Stopped,
    Running,
    /// The process ended; a new one is on its way
    Restarting,
    /// Given up on: it crashed too often
    Failed,
}

/// What the supervisor tells the rest of the backend on its own
pub enum Event {
    /// The plugin crashed too often; `why` is the last crash
    GaveUp { name: String, subname: String, why: String },
    /// A plugin that was given up on was started again, by its owner switching it on
    Revived { name: String, subname: String },
}

/// How the plugin is doing, for the Plugins page
#[derive(Serialize, ToSchema)]
pub struct Status {
    pub state: State,
    pub pid: Option<u32>,
    /// How many times a new process has been started after a crash
    pub restarts: u32,
    pub last_error: Option<String>,
    /// The end of what the plugin wrote to stderr or sent with `log`, oldest first
    pub logs: Vec<String>,
}

/// What a new process is told so that it is where the old one was
#[derive(Default)]
struct Replay {
    settings: Option<Value>,
    enabled: bool,
}

struct Health {
    state: State,
    restarts: u32,
    crashes: VecDeque<Instant>,
    last_error: Option<String>,
}

pub struct Supervisor {
    label: String,
    manifest: Manifest,
    plugin_dir: PathBuf,
    data_dir: PathBuf,
    db: Option<Arc<PluginDb>>,
    logs: Arc<LogRing>,
    current: RwLock<Option<Arc<PluginProcess>>>,
    replay: Mutex<Replay>,
    health: Mutex<Health>,
    timeouts_in_a_row: AtomicU32,
    /// Bumped by `run` and `stop`. A monitor belongs to the run it was started for and ends when that is no longer the current one,
    /// so a process that was stopped on purpose is never taken for a crash.
    run_id: AtomicU64,
    /// One start or stop at a time, and no restart in the middle of either: whoever starts or stops the plugin holds this across
    /// the whole sequence (`lifecycle`)
    lifecycle: tokio::sync::Mutex<()>,
    events: mpsc::UnboundedSender<Event>,
}

/// The pause before the next start when `crashes` have happened within the window; `None` when that is too many
fn backoff(crashes: usize) -> Option<Duration> {
    if crashes >= MAX_CRASHES {
        return None;
    }
    Some(BACKOFF[crashes.saturating_sub(1).min(BACKOFF.len() - 1)])
}

impl Supervisor {
    /// A plugin with no process yet; `run` starts it.
    pub fn new(manifest: Manifest, plugin_dir: PathBuf, data_dir: PathBuf, db: Option<Arc<PluginDb>>, events: mpsc::UnboundedSender<Event>) -> Arc<Supervisor> {
        Arc::new(Supervisor {
            label: format!("{}/{}", manifest.name, manifest.subname),
            manifest,
            plugin_dir,
            data_dir,
            db,
            logs: Arc::new(LogRing::default()),
            current: RwLock::new(None),
            replay: Mutex::new(Replay::default()),
            health: Mutex::new(Health { state: State::Stopped, restarts: 0, crashes: VecDeque::new(), last_error: None }),
            timeouts_in_a_row: AtomicU32::new(0),
            run_id: AtomicU64::new(0),
            lifecycle: tokio::sync::Mutex::new(()),
            events,
        })
    }

    /// Taken by whoever starts or stops the plugin, and held until the sequence is done (start, configure, enable; or disable,
    /// stop), so that two of them never overlap and a restart after a crash never lands in the middle.
    pub async fn lifecycle(&self) -> MutexGuard<'_, ()> {
        self.lifecycle.lock().await
    }

    /// Keeps why the plugin could not be switched on, for the Plugins page to show next to a plugin that is off
    pub fn record_error(&self, why: &str) {
        self.health.lock().unwrap().last_error = Some(why.to_string());
    }

    fn process(&self) -> Option<Arc<PluginProcess>> {
        self.current.read().unwrap().clone()
    }

    pub fn state(&self) -> State {
        self.health.lock().unwrap().state
    }

    pub fn status(&self) -> Status {
        let health = self.health.lock().unwrap();
        Status {
            state: health.state,
            pid: self.process().and_then(|p| p.pid()).filter(|_| health.state == State::Running),
            restarts: health.restarts,
            last_error: health.last_error.clone(),
            logs: self.logs.tail(50),
        }
    }

    /// A call to the plugin. It fails at once while the plugin has no process: switched off, being restarted, or given up on. What
    /// the call carries is noted when it works (the settings, the switch), so a new process can be brought to the same place.
    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        let Some(process) = self.process().filter(|_| self.state() == State::Running) else {
            return Err(CallError::Closed);
        };
        let result = process.call(method, params.clone(), timeout).await;
        match &result {
            Err(CallError::Timeout) => {
                if self.timeouts_in_a_row.fetch_add(1, Ordering::Relaxed) + 1 >= MAX_TIMEOUTS_IN_A_ROW {
                    // Stuck: ending it makes the monitor start a fresh one
                    tracing::warn!("plugin {}: {MAX_TIMEOUTS_IN_A_ROW} calls in a row got no answer, restarting it", self.label);
                    self.timeouts_in_a_row.store(0, Ordering::Relaxed);
                    process.kill();
                }
            }
            Err(CallError::Closed) => {}
            _ => {
                self.timeouts_in_a_row.store(0, Ordering::Relaxed);
                self.note(method, &params, result.is_ok());
            }
        }
        result
    }

    fn note(&self, method: &str, params: &Value, worked: bool) {
        let mut replay = self.replay.lock().unwrap();
        match method {
            "configure" if worked => replay.settings = params.get("settings").cloned(),
            "enable" if worked => replay.enabled = true,
            // Off whether or not the plugin heard of it: a plugin that is switched off is not switched on again by a restart
            "disable" => replay.enabled = false,
            _ => {}
        }
    }

    /// Starts the plugin's process if it has none that works (after it was switched off, or given up on) and watches it. The
    /// caller then configures it and switches it on. Needs the `lifecycle` guard.
    pub async fn run(self: &Arc<Self>, _held: &MutexGuard<'_, ()>) -> Result<(), String> {
        if self.state() == State::Running && self.process().is_some_and(|p| p.is_alive()) {
            return Ok(());
        }
        *self.replay.lock().unwrap() = Replay::default();
        let process = PluginProcess::start(&self.manifest, &self.plugin_dir, &self.data_dir, self.db.clone(), self.logs.clone()).await?;
        let id = self.run_id.fetch_add(1, Ordering::Relaxed) + 1;
        *self.current.write().unwrap() = Some(process);
        self.timeouts_in_a_row.store(0, Ordering::Relaxed);
        let was_failing = {
            let mut health = self.health.lock().unwrap();
            health.state = State::Running;
            health.crashes.clear();
            health.last_error.take().is_some()
        };
        if was_failing {
            let _ = self.events.send(Event::Revived { name: self.manifest.name.clone(), subname: self.manifest.subname.clone() });
        }
        tokio::spawn(self.clone().monitor(id));
        Ok(())
    }

    /// Ends the plugin's process: asked to stop, killed if it does not. Nothing happens if there is none. Needs the `lifecycle` guard.
    pub async fn stop(&self, _held: &MutexGuard<'_, ()>) {
        self.run_id.fetch_add(1, Ordering::Relaxed);
        let process = self.current.write().unwrap().take();
        {
            let mut health = self.health.lock().unwrap();
            health.state = State::Stopped;
        }
        *self.replay.lock().unwrap() = Replay::default();
        if let Some(process) = process {
            process.shutdown().await;
        }
    }

    /// Starts a process again after a crash, hands it the settings and the switch, and makes it the current one. `None` when the
    /// plugin was stopped or started by someone else in the meantime, and nothing is left running for it.
    async fn start_again(&self, id: u64) -> Option<Result<(), String>> {
        let _only_one = self.lifecycle.lock().await;
        if self.run_id.load(Ordering::Relaxed) != id {
            return None;
        }
        self.logs.push("-- starting the plugin again --".to_string());
        let process = match PluginProcess::start(&self.manifest, &self.plugin_dir, &self.data_dir, self.db.clone(), self.logs.clone()).await {
            Ok(process) => process,
            Err(e) => return Some(Err(e)),
        };
        let (settings, enabled) = {
            let replay = self.replay.lock().unwrap();
            (replay.settings.clone(), replay.enabled)
        };
        let handed = async {
            let step = |method: &'static str, params: Value| {
                let process = process.clone();
                async move {
                    match process.call(method, params, REPLAY_TIMEOUT).await {
                        Ok(_) => Ok(()),
                        // A plugin may leave a method out (see PROTOCOL.md)
                        Err(CallError::Rpc(e)) if e.code == METHOD_NOT_FOUND => Ok(()),
                        Err(e) => Err(format!("{method} failed after the restart: {e:?}")),
                    }
                }
            };
            if let Some(settings) = settings {
                step("configure", json!({"settings": settings})).await?;
            }
            if enabled {
                step("enable", json!({})).await?;
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(why) = handed {
            process.kill();
            return Some(Err(why));
        }
        *self.current.write().unwrap() = Some(process);
        self.timeouts_in_a_row.store(0, Ordering::Relaxed);
        Some(Ok(()))
    }

    fn note_crash(&self, why: &str) -> usize {
        let mut health = self.health.lock().unwrap();
        let now = Instant::now();
        health.crashes.push_back(now);
        while health.crashes.front().is_some_and(|at| now.duration_since(*at) > CRASH_WINDOW) {
            health.crashes.pop_front();
        }
        health.last_error = Some(why.to_string());
        health.crashes.len()
    }

    /// Waits for the process of run `id` to end; if nobody asked it to, starts it again until it stays up or the plugin is given
    /// up on.
    async fn monitor(self: Arc<Self>, id: u64) {
        loop {
            let Some(process) = self.process() else { return };
            let how = process.wait_exit().await;
            if self.run_id.load(Ordering::Relaxed) != id {
                return;
            }
            let tail = self.logs.tail(5).join(" | ");
            let mut why = format!("the process ended ({how})");
            if !tail.is_empty() {
                why = format!("{why}; it last wrote: {tail}");
            }
            tracing::warn!("plugin {}: {why}", self.label);
            let mut crashes = self.note_crash(&why);
            self.health.lock().unwrap().state = State::Restarting;
            loop {
                let Some(pause) = backoff(crashes) else {
                    let last = {
                        let mut health = self.health.lock().unwrap();
                        health.state = State::Failed;
                        health.last_error.clone().unwrap_or_else(|| why.clone())
                    };
                    tracing::error!("plugin {}: crashed {crashes} times within {} minutes, giving up on it", self.label, CRASH_WINDOW.as_secs() / 60);
                    let _ = self.events.send(Event::GaveUp { name: self.manifest.name.clone(), subname: self.manifest.subname.clone(), why: last });
                    return;
                };
                tokio::time::sleep(pause).await;
                match self.start_again(id).await {
                    // Stopped, or started by someone else, while this waited
                    None => return,
                    Some(Ok(())) => {
                        let mut health = self.health.lock().unwrap();
                        health.state = State::Running;
                        health.restarts += 1;
                        tracing::info!("plugin {}: running again", self.label);
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::warn!("plugin {}: could not be started again: {e}", self.label);
                        crashes = self.note_crash(&e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pause_grows_with_the_crashes_and_the_plugin_is_given_up_on_after_five() {
        let pauses: Vec<Option<u64>> = (1..=6).map(|n| backoff(n).map(|d| d.as_secs())).collect();
        assert_eq!(pauses, [Some(1), Some(5), Some(30), Some(30), None, None]);
    }
}
