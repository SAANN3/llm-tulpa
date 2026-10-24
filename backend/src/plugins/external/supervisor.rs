//! Keeps one external plugin's process running. Every call to the plugin goes through here, so the rest of the backend holds a
//! `Supervisor` and never a process: when the process ends without having been asked to, a new one is started (after a
//! growing pause), told the settings and whether the plugin is switched on, and the calls that were refused meanwhile work
//! again. A plugin that keeps crashing is given up on and an event says so, so the registry can switch it off.
//!
//! The tools of a plugin that is being restarted stay registered. Taking them out and putting them back would change the
//! tool list the model is sent, for a pause of a second or so, and a call in that time fails with a message that says the
//! plugin is restarting.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
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
    current: RwLock<Arc<PluginProcess>>,
    replay: Mutex<Replay>,
    health: Mutex<Health>,
    timeouts_in_a_row: AtomicU32,
    /// Set when the backend (or the owner) ends the plugin: its process going away is then no crash
    closing: AtomicBool,
    /// One start at a time: the monitor after a crash and `revive` must not both start one
    starting: tokio::sync::Mutex<()>,
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
    /// Starts the plugin's process; it answers for the plugin from then on.
    pub async fn start(
        manifest: Manifest,
        plugin_dir: PathBuf,
        data_dir: PathBuf,
        db: Option<Arc<PluginDb>>,
        events: mpsc::UnboundedSender<Event>,
    ) -> Result<Arc<Supervisor>, String> {
        let logs = Arc::new(LogRing::default());
        let process = PluginProcess::start(&manifest, &plugin_dir, &data_dir, db.clone(), logs.clone()).await?;
        let supervisor = Arc::new(Supervisor {
            label: format!("{}/{}", manifest.name, manifest.subname),
            manifest,
            plugin_dir,
            data_dir,
            db,
            logs,
            current: RwLock::new(process),
            replay: Mutex::new(Replay::default()),
            health: Mutex::new(Health { state: State::Running, restarts: 0, crashes: VecDeque::new(), last_error: None }),
            timeouts_in_a_row: AtomicU32::new(0),
            closing: AtomicBool::new(false),
            starting: tokio::sync::Mutex::new(()),
            events,
        });
        tokio::spawn(supervisor.clone().monitor());
        Ok(supervisor)
    }

    fn process(&self) -> Arc<PluginProcess> {
        self.current.read().unwrap().clone()
    }

    pub fn state(&self) -> State {
        self.health.lock().unwrap().state
    }

    pub fn status(&self) -> Status {
        let health = self.health.lock().unwrap();
        Status {
            state: health.state,
            pid: self.process().pid().filter(|_| health.state == State::Running),
            restarts: health.restarts,
            last_error: health.last_error.clone(),
            logs: self.logs.tail(50),
        }
    }

    /// A call to the plugin. It fails at once while the plugin is being restarted or given up on. What the call carries is
    /// noted when it works (the settings, the switch), so a new process can be brought to the same place.
    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        if self.state() != State::Running {
            return Err(CallError::Closed);
        }
        let process = self.process();
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

    /// Ends the plugin for good (the backend is stopping, or the plugin is unloaded)
    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Relaxed);
        self.process().shutdown().await;
    }

    /// Starts a new process if the plugin has none that works: after the supervisor gave up on it, or when it was found dead
    /// before the monitor noticed. Switching a failed plugin on again goes through here. A plugin being restarted already is
    /// not started twice.
    pub async fn revive(self: &Arc<Self>) -> Result<(), String> {
        if self.closing.load(Ordering::Relaxed) {
            return Err("the plugin was unloaded".to_string());
        }
        let was = self.state();
        match was {
            State::Running if self.process().is_alive() => return Ok(()),
            State::Restarting => return Err("it is being restarted after a crash; try again in a moment".to_string()),
            _ => {}
        }
        self.start_process().await?;
        {
            let mut health = self.health.lock().unwrap();
            health.state = State::Running;
            health.crashes.clear();
            health.last_error = None;
        }
        // The monitor ended when it gave up; one that is still there is watching and moves on to the new process by itself
        if was == State::Failed {
            tokio::spawn(self.clone().monitor());
            let _ = self.events.send(Event::Revived { name: self.manifest.name.clone(), subname: self.manifest.subname.clone() });
        }
        Ok(())
    }

    /// Starts a process, hands it the settings and the switch, and makes it the current one.
    async fn start_process(&self) -> Result<(), String> {
        let _only_one = self.starting.lock().await;
        self.logs.push("-- starting the plugin again --".to_string());
        let process = PluginProcess::start(&self.manifest, &self.plugin_dir, &self.data_dir, self.db.clone(), self.logs.clone()).await?;
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
            return Err(why);
        }
        *self.current.write().unwrap() = process;
        self.timeouts_in_a_row.store(0, Ordering::Relaxed);
        Ok(())
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

    /// Waits for the current process to end; if nobody asked it to, restarts it until it stays up or the plugin is given up on.
    async fn monitor(self: Arc<Self>) {
        loop {
            let process = self.process();
            let how = process.wait_exit().await;
            if self.closing.load(Ordering::Relaxed) {
                return;
            }
            // `revive` may have put a new process in place of the one this was waiting for
            if !Arc::ptr_eq(&process, &self.process()) {
                continue;
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
                if self.closing.load(Ordering::Relaxed) {
                    return;
                }
                match self.start_process().await {
                    Ok(()) => {
                        let mut health = self.health.lock().unwrap();
                        health.state = State::Running;
                        health.restarts += 1;
                        tracing::info!("plugin {}: running again", self.label);
                        break;
                    }
                    Err(e) => {
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
