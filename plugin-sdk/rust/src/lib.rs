//! SDK for writing an llm-tulpa plugin as a separate program in Rust. The wire format is `plugin-sdk/PROTOCOL.md`; this
//! crate is a convenience over it, so a plugin is one trait and a `main` and never writes protocol code.
//!
//! ```no_run
//! use tulpa_plugin::{async_trait, CallContext, Plugin, ToolFailure, ToolSpec, Value, json};
//!
//! struct Hello;
//!
//! #[async_trait]
//! impl Plugin for Hello {
//!     fn tools(&self) -> Vec<ToolSpec> { vec![ToolSpec::new("hello.greet", "Greets the user")] }
//!     async fn call_tool(&self, _tool: &str, _args: Value, _ctx: CallContext) -> Result<Value, ToolFailure> {
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
mod spec;
mod stdio;

pub use async_trait::async_trait;
pub use serde_json::{Value, json};
pub use spec::{Param, ParamType, ToolSpec};

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

/// What kind of failure a tool call was, as the protocol names them
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The arguments were wrong
    BadArgs,
    /// What the call asked for does not exist
    NotFound,
    /// Anything else
    Internal,
}

/// Why a tool call failed. The text goes back to the model, so say what went wrong in a way it can act on. A plain string
/// converts to an `Internal` failure.
#[derive(Clone, Debug)]
pub struct ToolFailure {
    pub kind: FailureKind,
    pub message: String,
}

impl ToolFailure {
    pub fn bad_args(message: impl Into<String>) -> Self {
        Self { kind: FailureKind::BadArgs, message: message.into() }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self { kind: FailureKind::NotFound, message: message.into() }
    }
}

impl From<String> for ToolFailure {
    fn from(message: String) -> Self {
        Self { kind: FailureKind::Internal, message }
    }
}

impl From<&str> for ToolFailure {
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

/// The plugin, as the backend sees it. What the plugin is (name, version, summary, help text, whether it starts switched on)
/// is in `plugin.toml`; the program only has its tools. Everything but `call_tool` has a default.
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// The tools the model can call
    fn tools(&self) -> Vec<ToolSpec> {
        vec![]
    }

    /// Runs one tool. `tool` is one of the names `tools` listed and `args` the arguments the model gave.
    async fn call_tool(&self, tool: &str, args: Value, ctx: CallContext) -> Result<Value, ToolFailure>;
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
