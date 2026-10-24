//! SDK for writing an llm-tulpa plugin as a separate program in Rust. The wire format is `plugin-sdk/PROTOCOL.md`; this
//! crate is a convenience over it, so a plugin is one trait and a `main` and never writes protocol code.
//!
//! ```no_run
//! use tulpa_plugin::{async_trait, CallContext, Plugin, Failure, ToolSpec, Value, json};
//!
//! struct Hello;
//!
//! #[async_trait]
//! impl Plugin for Hello {
//!     fn tools(&self) -> Vec<ToolSpec> { vec![ToolSpec::new("hello.greet", "Greets the user")] }
//!     async fn call_tool(&self, _tool: &str, _args: Value, _ctx: CallContext) -> Result<Value, Failure> {
//!         Ok(json!("hello"))
//!     }
//! }
//!
//! fn main() -> Result<(), tulpa_plugin::Error> {
//!     tulpa_plugin::serve(Hello)
//! }
//! ```
//!
//! A plugin talks to the backend over its stdin and stdout, so a stray `println!` would corrupt the conversation. `serve`
//! points the program's standard output at its standard error (on Unix) and keeps the real one for the protocol; whatever
//! the plugin prints then lands in the log the backend shows. Use `eprintln!` or `Host::log` on purpose all the same: on
//! other systems nothing redirects a print.

mod connection;
mod http;
mod spec;
mod stdio;
mod tab;

pub use async_trait::async_trait;
pub use serde_json::{Value, json};
pub use http::{HttpRequest, HttpResponse};
pub use spec::{Param, ParamType, ToolSpec};
pub use tab::{Block, Row, Section, Tab};

use std::fmt;
use std::sync::Arc;

use connection::Connection;

/// Which protocol this SDK speaks
pub const PROTOCOL: u32 = 1;

/// What the backend tells a call about where it comes from
#[derive(Clone, Debug)]
pub struct CallContext {
    /// The user whose chat the call is in. A tool works on this user's data only.
    pub user_id: i64,
    pub chat_id: i64,
    pub model: String,
    pub provider: String,
    pub host: Host,
}

/// What kind of failure a call was, as the protocol names them
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The arguments (or the settings) were wrong
    BadArgs,
    /// What the call asked for does not exist
    NotFound,
    /// Anything else
    Internal,
}

/// Why a call failed. The text goes back to whoever asked: the model for a tool, the owner for settings, the user for a tab
/// action, so say what went wrong in a way they can act on. A plain string converts to an `Internal` failure.
#[derive(Clone, Debug)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}

impl Failure {
    pub fn bad_args(message: impl Into<String>) -> Self {
        Self { kind: FailureKind::BadArgs, message: message.into() }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self { kind: FailureKind::NotFound, message: message.into() }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self { kind: FailureKind::Internal, message }
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

/// Why the plugin could not run
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    fn word(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }
}

/// A file the user gave a tab, already written to disk: `path` is the path it had in the user's browser (a dropped folder
/// keeps its shape) and `file` where it is now. The folder holding it is removed when `tab_upload` returns, so copy or move
/// what you keep.
#[derive(Clone, Debug)]
pub struct UploadedFile {
    pub path: String,
    pub file: std::path::PathBuf,
}

/// The plugin, as the backend sees it. What the plugin is (name, version, summary, help text, its settings, whether it has a
/// tab and starts switched on) is in `plugin.toml`; the program is what the plugin does. Everything but `call_tool` has a
/// default, so a plugin implements the hooks it uses.
///
/// The backend calls the hooks on `&self` from several tasks at once, so state that changes (the settings, say) lives behind
/// a lock.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// The owner saved the plugin's settings, which the backend has already checked against `[[settings]]` in the manifest.
    /// It runs before the plugin is switched on and again on every change. `host` is for logging from the other hooks, which
    /// do not get one: keep it. Refuse settings only you can judge (a token the service rejects) with `Failure::bad_args`;
    /// the owner sees the text.
    async fn configure(&self, _settings: Value, _host: Host) -> Result<(), Failure> {
        Ok(())
    }

    /// The tools the model can call. Asked for after every `configure`, so they may depend on the settings.
    fn tools(&self) -> Vec<ToolSpec> {
        vec![]
    }

    /// Runs one tool. `tool` is one of the names `tools` listed and `args` the arguments the model gave.
    async fn call_tool(&self, tool: &str, args: Value, ctx: CallContext) -> Result<Value, Failure>;

    /// The plugin was switched on: start the background work it does, if any.
    async fn enable(&self) -> Result<(), Failure> {
        Ok(())
    }

    /// The plugin was switched off, or its settings are about to change: stop what `enable` started.
    async fn disable(&self) -> Result<(), Failure> {
        Ok(())
    }

    /// A text added to the system prompt of `user_id`'s chats while the plugin is on, `None` for nothing. `tools` says whether
    /// the chat sends the model its tools. It is asked for every turn and has 5 s to answer, and it must be the same text each
    /// time while what it describes is the same: any change makes the model server read the whole chat again.
    async fn prompt_section(&self, _user_id: i64, _tools: bool) -> Option<String> {
        None
    }

    /// What the plugin's tab shows `user_id`. Needs `[tab]` in the manifest.
    async fn tab_layout(&self, _user_id: i64) -> Result<Tab, Failure> {
        Err(Failure::not_found("this plugin has no tab"))
    }

    /// What `user_id` did in the tab: `action` is the word the control carries and `args` what it sent (`{"on": bool}` for a
    /// switch, `{"row": id}` for a row's button). Answer with the tab as it is after.
    async fn tab_action(&self, _user_id: i64, _action: &str, _args: Value) -> Result<Tab, Failure> {
        Err(Failure::not_found("this plugin has no tab"))
    }

    /// Files `user_id` dropped on a drop zone (`action` is the zone's), or chose with a row's upload button (then `row` is
    /// that row's id). Answer with the tab as it is after.
    async fn tab_upload(&self, _user_id: i64, _action: &str, _row: Option<String>, _files: Vec<UploadedFile>) -> Result<Tab, Failure> {
        Err(Failure::not_found("this plugin has no tab"))
    }

    /// A request to `/api/plugins/<name>/<subname>/...`
    async fn http_request(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::status(404)
    }

    /// `user_id` was deleted: remove what you keep for them outside the backend's database (files, say). The user is gone
    /// whatever happens here, so a failure is yours to log.
    async fn user_deleted(&self, _user_id: i64) {}
}

/// A handle for what the plugin asks of the backend
#[derive(Clone)]
pub struct Host {
    conn: Arc<Connection>,
}

impl fmt::Debug for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Host")
    }
}

impl Host {
    /// A line in the backend's log and on the plugin's page
    pub async fn log(&self, level: Level, message: &str) {
        // A log line that cannot be delivered is not worth failing the tool call for
        let _ = self.conn.call("log", json!({"level": level.word(), "message": message})).await;
    }

    /// The folder that is the plugin's own for files; the backend does not create it
    pub fn data_dir(&self) -> Option<std::path::PathBuf> {
        self.conn.host_info().and_then(|info| info["data_dir"].as_str().map(Into::into))
    }
}

/// Runs the plugin until the backend ends it: the entry point of a plugin's `main`. Builds its own runtime and protects the
/// protocol's stdout (see the crate documentation).
pub fn serve(plugin: impl Plugin) -> Result<(), Error> {
    let (stdin, stdout) = stdio::claim();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error(format!("cannot start the runtime: {e}")))?
        .block_on(connection::run(Arc::new(plugin), stdin, stdout))
}
