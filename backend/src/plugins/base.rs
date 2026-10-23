use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::http::StatusCode;
use serde_json::Value;

use crate::services::error::ErrorService;
use crate::services::tools::ToolServiceError;
use crate::tools::base::{PropertyInfo, Tool};

use super::env::PluginEnv;
use super::tab::{TabFile, TabInfo, TabLayout};

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

    /// Tools this plugin contributes to the agent's tool set. Called on `on_enabled`;
    /// the returned tools are registered in `ToolService` by name and removed again
    /// on `on_disabled`. Defaults to an empty list — plugins that only serve API routes
    /// or run background work don't need to override this.
    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![]
    }

    /// A text this plugin adds to the system prompt of `user_id`'s chats while it is enabled, `None` for nothing: what
    /// differs from one user to the next (a user's own skills, say) can be in it. `tools` says whether the chat sends
    /// the model its tools, so a section about the plugin's tools can leave itself out of a chat that has none. It must be
    /// the same text every time it is asked while nothing it describes has changed: it sits in the prompt every turn, and
    /// any change makes the model server read the whole chat again. It is used as it is, with no limit on its length:
    /// enabling the plugin is the user's choice.
    async fn prompt_section(&self, _user_id: i64, _tools: bool) -> Option<String> {
        None
    }

    /// A tab of this plugin's own in Settings, shown to every user while the plugin is enabled; `None` for none. What it
    /// holds comes from `tab_layout`, see `plugins::tab`.
    fn settings_tab(&self) -> Option<TabInfo> {
        None
    }

    /// What the plugin's tab shows `user_id` now.
    async fn tab_layout(&self, _user_id: i64) -> Result<TabLayout, PluginError> {
        Err(PluginError::NotFound("this plugin has no tab".to_string()))
    }

    /// What `user_id` did in the tab (a switch, a button): `action` is the word the control carries and `args` what it
    /// sent. Answers with the layout as it is after.
    async fn tab_action(&self, _user_id: i64, _action: &str, _args: Value) -> Result<TabLayout, PluginError> {
        Err(PluginError::NotFound("this plugin has no tab".to_string()))
    }

    /// Files `user_id` dropped on a drop zone of the tab, with the zone's `action`. Answers with the layout as it is after.
    async fn tab_upload(&self, _user_id: i64, _action: &str, _files: Vec<TabFile>) -> Result<TabLayout, PluginError> {
        Err(PluginError::NotFound("this plugin has no tab".to_string()))
    }
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

    /// What the plugin does, in one short line: shown under its name in the plugin list.
    fn summary(&self) -> &str;

    /// A human-readable info message for this plugin type — e.g. Telegram's explains
    /// how to find/talk to the bot, step by step. Shown as-is on the frontend's plugin
    /// settings panel, same static-per-type nature as `settings_schema` (not per
    /// instance, doesn't depend on current settings).
    fn help_message(&self) -> String;

    /// `env` is what the host gives this plugin (see `plugins::env`): the database, the agent, and a folder of its own
    /// for files. A builder lives as long as the process and `build` runs again on every settings change, so what must
    /// survive a rebuild (a lock, say) is kept in the builder, made from `env` on the first call.
    async fn build(&self, settings: Value, env: &PluginEnv) -> Result<Arc<dyn Plugin>, PluginError>;

    /// The SQL this plugin needs in the database, as ordered steps; `prefix` is the start every table and index it
    /// creates must have (`plugin_<name>_<subname>`, so `format!("CREATE TABLE {prefix}_items (...)")`). The registry
    /// applies the steps a database has not seen yet before the plugin is registered, in order, each in a transaction of
    /// its own, whether or not the plugin is enabled (what it stored has to be there). Steps only add: CREATE TABLE,
    /// CREATE INDEX, ALTER TABLE ... ADD, INSERT and UPDATE of the plugin's own tables, no drops and no deletes. Add a step
    /// to change something; never edit or reorder one that has been released, the database only counts how many it has
    /// applied. A foreign key to a core table (`REFERENCES users (id) ON DELETE CASCADE`) is fine. See
    /// `plugins::schema`. None by default.
    fn migrations(&self, _prefix: &str) -> Vec<String> {
        vec![]
    }
}

/// Why building or running a plugin failed. Mirrors `ToolError`'s shape for the same
/// reason: `Deserialization` covers settings JSON that doesn't match what this plugin
/// expects, `FailedUnknown` is a catch-all until a plugin needs a more specific variant.
/// `NotFound` and `InvalidState` are their own variants (rather than folded into
/// `FailedUnknown`) because both are client mistakes — a bad `plugin_name`/
/// `plugin_subname`, or a request that doesn't make sense given the plugin's current
/// state (e.g. enabling one with no settings yet) — not internal failures;
/// `From<PluginError> for ErrorService` below needs to tell those apart from
/// `FailedUnknown` to pick the right HTTP status. `ToolConflict` is a client mistake
/// of a different kind: the plugin tries to register a tool whose name is already taken.
#[derive(Debug)]
pub enum PluginError {
    Deserialization(serde_json::Error),
    NotFound(String),
    InvalidState(String),
    FailedUnknown(String),
    /// A tool exported by this plugin collides with an already-registered tool name.
    ToolConflict(String),
    /// The plugin's own tables (`PluginBuilder::migrations`) could not be brought up to date.
    Schema(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Deserialization(e) => write!(f, "invalid plugin settings, err = {e}"),
            PluginError::NotFound(reason) => write!(f, "plugin not found: {reason}"),
            PluginError::InvalidState(reason) => write!(f, "plugin error: {reason}"),
            PluginError::FailedUnknown(reason) => write!(f, "plugin error: {reason}"),
            PluginError::ToolConflict(name) => {
                write!(f, "plugin tool '{name}' conflicts with an already-registered tool")
            }
            PluginError::Schema(reason) => write!(f, "plugin database error: {reason}"),
        }
    }
}

impl From<PluginError> for ErrorService {
    fn from(err: PluginError) -> Self {
        match err {
            PluginError::Deserialization(e) => ErrorService::new(StatusCode::BAD_REQUEST, format!("invalid plugin settings: {e}")),
            PluginError::NotFound(reason) => ErrorService::new(StatusCode::NOT_FOUND, reason),
            PluginError::InvalidState(reason) => ErrorService::new(StatusCode::BAD_REQUEST, reason),
            PluginError::ToolConflict(name) => ErrorService::new(
                StatusCode::CONFLICT,
                format!("cannot enable plugin: tool '{name}' is already registered by another plugin or built-in tool"),
            ),
            PluginError::FailedUnknown(reason) => {
                tracing::error!("plugin error: {reason}");
                ErrorService::internal(reason)
            }
            PluginError::Schema(reason) => {
                tracing::error!("plugin database error: {reason}");
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

impl From<ToolServiceError> for PluginError {
    fn from(e: ToolServiceError) -> Self {
        match e {
            ToolServiceError::Collision(name) => PluginError::ToolConflict(name),
        }
    }
}
