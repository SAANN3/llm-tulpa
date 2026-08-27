use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::http::StatusCode;
use serde_json::Value;

use crate::services::error::ErrorService;
use crate::tools::base::PropertyInfo;

/// One running instance of a plugin — e.g. the VK provider under the "messaging"
/// plugin type. `plugin_name` identifies the type/contract this instance shares with
/// its siblings (same API shape, same settings shape); `plugin_subname` identifies
/// which concrete implementation this particular instance is. A plugin type with
/// several interchangeable subplugins (VK, Telegram under "messaging") is expected to
/// get one shared generic impl of this trait, parameterized over a per-type domain
/// trait (see `plugins/messaging.rs` once it exists) — subplugins themselves never
/// implement `Plugin` directly, so `plugin_name` never has to be redeclared per
/// subplugin.
#[async_trait]
pub trait Plugin: Send + Sync {
    fn plugin_name(&self) -> &str;
    fn plugin_subname(&self) -> &str;

    /// Current settings as JSON, for prefilling the settings form on the frontend.
    /// There's no in-place `apply_settings` counterpart — a settings change is handled
    /// by discarding this instance and building a fresh one via `PluginBuilder::build`
    /// instead (see there for why).
    fn settings_value(&self) -> Value;

    /// Routes this plugin serves under `/api/plugins/{plugin_name}/{plugin_subname}`.
    /// Deliberately a plain `Router` (state `()`), not `Router<Arc<AppState>>` like the
    /// rest of `routes/` — a plugin's handlers need this instance's own captured state
    /// (its provider), not the global `AppState`, so it should build its handlers with
    /// `.with_state(...)` internally and hand back something already stateless. A
    /// stateless `Router` nests into any `Router<S>` regardless of `S`.
    fn api_router(&self) -> Router;

    /// Called when this plugin instance transitions to enabled — the place to start
    /// whatever background work it does (e.g. a poll loop), if any.
    async fn on_enabled(&self) -> Result<(), PluginError>;

    /// Called when this plugin instance transitions to disabled — must actually stop
    /// any background work started in `on_enabled`, not just report itself as off.
    async fn on_disabled(&self) -> Result<(), PluginError>;
}

/// Builds a `Plugin` instance from settings JSON. Kept separate from `Plugin` itself
/// since a trait object can't have an associated constructor. Used both to build a
/// plugin the first time and to rebuild it whenever its settings change — this project
/// doesn't do in-place settings updates, a change is handled as "tear down the old
/// instance, build a new one from the new settings" instead.
#[async_trait]
pub trait PluginBuilder: Send + Sync {
    fn plugin_name(&self) -> &str;
    fn plugin_subname(&self) -> &str;

    /// The settings this plugin type needs, in the same shape `ToolParams` produces
    /// for tool arguments — reused here so the frontend can render a settings form
    /// generically from this schema instead of needing hand-built UI per plugin.
    fn settings_schema(&self) -> Vec<PropertyInfo>;

    /// A human-readable info message for this plugin type — e.g. Telegram's explains
    /// how to find/talk to the bot, step by step. Shown as-is on the frontend's plugin
    /// settings panel, same static-per-type nature as `settings_schema` (not per
    /// instance, doesn't depend on current settings).
    fn help_message(&self) -> String;

    async fn build(&self, settings: Value) -> Result<Arc<dyn Plugin>, PluginError>;
}

/// Why building or running a plugin failed. Mirrors `ToolError`'s shape for the same
/// reason: `Deserialization` covers settings JSON that doesn't match what this plugin
/// expects, `FailedUnknown` is a catch-all until a plugin needs a more specific variant.
/// `NotFound` and `InvalidState` are their own variants (rather than folded into
/// `FailedUnknown`) because both are client mistakes — a bad `plugin_name`/
/// `plugin_subname`, or a request that doesn't make sense given the plugin's current
/// state (e.g. enabling one with no settings yet) — not internal failures;
/// `From<PluginError> for ErrorService` below needs to tell those apart from
/// `FailedUnknown` to pick the right HTTP status.
#[derive(Debug)]
pub enum PluginError {
    Deserialization(serde_json::Error),
    NotFound(String),
    InvalidState(String),
    FailedUnknown(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Deserialization(e) => write!(f, "invalid plugin settings, err = {e}"),
            PluginError::NotFound(reason) => write!(f, "plugin not found: {reason}"),
            PluginError::InvalidState(reason) => write!(f, "plugin error: {reason}"),
            PluginError::FailedUnknown(reason) => write!(f, "plugin error: {reason}"),
        }
    }
}

impl From<PluginError> for ErrorService {
    fn from(err: PluginError) -> Self {
        match err {
            PluginError::Deserialization(e) => ErrorService::new(StatusCode::BAD_REQUEST, format!("invalid plugin settings: {e}")),
            PluginError::NotFound(reason) => ErrorService::new(StatusCode::NOT_FOUND, reason),
            PluginError::InvalidState(reason) => ErrorService::new(StatusCode::BAD_REQUEST, reason),
            PluginError::FailedUnknown(reason) => {
                tracing::error!("plugin error: {reason}");
                ErrorService::internal(reason)
            }
        }
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        PluginError::Deserialization(e)
    }
}

impl From<crate::services::plugin_settings_store::PluginSettingsStoreErrors> for PluginError {
    fn from(e: crate::services::plugin_settings_store::PluginSettingsStoreErrors) -> Self {
        PluginError::FailedUnknown(format!("plugin settings store error: {e:?}"))
    }
}
