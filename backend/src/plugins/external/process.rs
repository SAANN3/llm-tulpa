//! One running external plugin: the child process, the connection to it and what it has logged. `start` spawns it and
//! brings it through the opening of the protocol (the plugin's `hello`, which names the protocol it speaks); after that the
//! backend calls it with `call` and ends it with `shutdown`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;

use super::db::{PluginDb, Sql};
use super::manifest::{Manifest, SUPPORTED_PROTOCOL, expand_command};
use super::rpc::{BAD_ARGS, CallError, INTERNAL, Incoming, METHOD_NOT_FOUND, RpcClient, RpcError, read_line};

/// How long a plugin has to say `hello` after it starts
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `shutdown` waits for the plugin to answer and then for the process to exit, each, before it is killed
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// Lines of stderr (and `log` calls) kept for showing on the Plugins page
const LOG_LINES: usize = 200;
/// A longer stderr line is dropped rather than kept
const MAX_LOG_LINE_BYTES: usize = 64 * 1024;

/// The last lines a plugin wrote to stderr or sent with `log`, oldest first
#[derive(Default)]
pub struct LogRing(Mutex<VecDeque<String>>);

impl LogRing {
    fn push(&self, line: String) {
        let mut lines = self.0.lock().unwrap();
        if lines.len() == LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// The newest `n` lines, oldest first
    pub fn tail(&self, n: usize) -> Vec<String> {
        let lines = self.0.lock().unwrap();
        lines.iter().skip(lines.len().saturating_sub(n)).cloned().collect()
    }
}

#[derive(Deserialize)]
struct Hello {
    protocol: u32,
}

/// The host's side of the conversation: answers `hello` and `log`, the calls every plugin may make
struct HostSide {
    label: String,
    info: Value,
    logs: Arc<LogRing>,
    /// `None` for a plugin whose manifest has no `[database]`
    db: Option<Arc<PluginDb>>,
    hello: Mutex<Option<oneshot::Sender<Result<(), String>>>>,
}

#[async_trait]
impl Incoming for HostSide {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "hello" => {
                let verdict = self.check_hello(params);
                let first = self.hello.lock().unwrap().take();
                match (first, verdict) {
                    (Some(tx), Ok(())) => {
                        let _ = tx.send(Ok(()));
                        Ok(self.info.clone())
                    }
                    (Some(tx), Err(why)) => {
                        let _ = tx.send(Err(why.clone()));
                        Err(RpcError::new(BAD_ARGS, why))
                    }
                    (None, _) => Err(RpcError::new(BAD_ARGS, "hello was already sent")),
                }
            }
            "log" => {
                let message = params.get("message").and_then(Value::as_str).unwrap_or_default();
                match params.get("level").and_then(Value::as_str).unwrap_or("info") {
                    "error" => tracing::error!("plugin {}: {message}", self.label),
                    "warn" => tracing::warn!("plugin {}: {message}", self.label),
                    "debug" => tracing::debug!("plugin {}: {message}", self.label),
                    _ => tracing::info!("plugin {}: {message}", self.label),
                }
                self.logs.push(message.to_string());
                Ok(Value::Null)
            }
            "db.query" | "db.execute" | "db.batch" => self.database(method, params).await,
            _ => Err(RpcError::new(METHOD_NOT_FOUND, format!("the host has no method {method}"))),
        }
    }
}

impl HostSide {
    async fn database(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let db = self.db.as_ref().ok_or_else(|| RpcError::new(BAD_ARGS, "this plugin has no database: add [database] to plugin.toml"))?;
        let bad = |e: serde_json::Error| RpcError::new(BAD_ARGS, format!("{method}: {e}"));
        let failed = |why: String| RpcError::new(INTERNAL, why);
        match method {
            "db.query" => db.query(serde_json::from_value::<Sql>(params).map_err(bad)?).await.map_err(failed),
            "db.execute" => Ok(json!(db.execute(serde_json::from_value::<Sql>(params).map_err(bad)?).await.map_err(failed)?)),
            _ => {
                #[derive(Deserialize)]
                struct Batch {
                    statements: Vec<Sql>,
                }
                let batch: Batch = serde_json::from_value(params).map_err(bad)?;
                Ok(json!(db.batch(batch.statements).await.map_err(failed)?))
            }
        }
    }

    fn check_hello(&self, params: Value) -> Result<(), String> {
        let hello: Hello = serde_json::from_value(params).map_err(|e| format!("hello is malformed: {e}"))?;
        if hello.protocol != SUPPORTED_PROTOCOL {
            return Err(format!("it speaks protocol {}, this backend speaks {SUPPORTED_PROTOCOL}", hello.protocol));
        }
        Ok(())
    }
}

pub struct PluginProcess {
    rpc: RpcClient,
    child: tokio::sync::Mutex<Child>,
    logs: Arc<LogRing>,
}

impl PluginProcess {
    /// Spawns the plugin's `run` command in its folder and waits for it to say `hello`. A plugin that does not, or whose
    /// `hello` names a protocol this backend does not speak, is killed and the reason returned along with the end of its stderr.
    pub async fn start(manifest: &Manifest, plugin_dir: &Path, data_dir: &Path, db: Option<Arc<PluginDb>>) -> Result<Arc<PluginProcess>, String> {
        let label = format!("{}/{}", manifest.name, manifest.subname);
        let words = expand_command(&manifest.run_command(), plugin_dir, data_dir).map_err(|e| e.to_string())?;
        let (program, args) = words.split_first().ok_or("the run command is empty")?;
        // A relative program with a slash is meant to be inside the plugin's folder (`.venv/bin/python`); a bare name is
        // looked up on PATH as usual
        let program = if program.contains('/') && Path::new(program).is_relative() { plugin_dir.join(program) } else { PathBuf::from(program) };

        let mut child = Command::new(&program)
            .args(args)
            .current_dir(plugin_dir)
            .env("TULPA_PLUGIN_PROTOCOL", SUPPORTED_PROTOCOL.to_string())
            .env("TULPA_DATA_DIR", data_dir)
            .env("TULPA_PLUGIN_DIR", plugin_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // The backend ending any other way than a clean shutdown still takes its plugins with it
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", program.display()))?;

        let logs = Arc::new(LogRing::default());
        let stderr = child.stderr.take().ok_or("no stderr")?;
        tokio::spawn(copy_stderr(stderr, logs.clone(), label.clone()));

        let (tx, rx) = oneshot::channel();
        let host = Arc::new(HostSide {
            label: label.clone(),
            info: json!({
                "protocol": SUPPORTED_PROTOCOL,
                "tulpa_version": env!("CARGO_PKG_VERSION"),
                "data_dir": data_dir,
                "plugin_dir": plugin_dir,
            }),
            logs: logs.clone(),
            db,
            hello: Mutex::new(Some(tx)),
        });
        let rpc = RpcClient::start(child.stdout.take().ok_or("no stdout")?, child.stdin.take().ok_or("no stdin")?, host, label);

        let opened = match tokio::time::timeout(HELLO_TIMEOUT, rx).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(why))) => Err(why),
            Ok(Err(_)) => Err("it ended before it said hello".to_string()),
            Err(_) => Err(format!("it did not say hello within {} s", HELLO_TIMEOUT.as_secs())),
        };
        let process = Arc::new(PluginProcess { rpc, child: tokio::sync::Mutex::new(child), logs });
        if let Err(why) = opened {
            // It never got as far as listening for a shutdown, so there is nothing to wait for
            process.kill().await;
            // Stderr is read on another task: a moment lets the last lines of a crash arrive
            tokio::time::sleep(Duration::from_millis(100)).await;
            let tail = process.logs.tail(10).join("\n");
            return Err(if tail.is_empty() { why } else { format!("{why}; it wrote:\n{tail}") });
        }
        Ok(process)
    }

    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        self.rpc.call(method, params, timeout).await
    }

    /// Asks the plugin to stop, closes its stdin, and kills it if it is still there after the grace period
    pub async fn shutdown(&self) {
        let _ = self.rpc.call("shutdown", json!({}), SHUTDOWN_GRACE).await;
        self.rpc.close();
        let mut child = self.child.lock().await;
        if tokio::time::timeout(SHUTDOWN_GRACE, child.wait()).await.is_err() {
            let _ = child.kill().await;
        }
    }

    async fn kill(&self) {
        self.rpc.close();
        let _ = self.child.lock().await.kill().await;
    }
}

async fn copy_stderr(stderr: tokio::process::ChildStderr, logs: Arc<LogRing>, label: String) {
    let mut reader = BufReader::new(stderr);
    loop {
        match read_line(&mut reader, MAX_LOG_LINE_BYTES).await {
            Ok(Some(Ok(line))) => {
                let line = String::from_utf8_lossy(&line).into_owned();
                tracing::debug!("plugin {label}: {line}");
                logs.push(line);
            }
            Ok(Some(Err(()))) => logs.push("(a line over 64 KiB was dropped)".to_string()),
            Ok(None) | Err(_) => break,
        }
    }
}
