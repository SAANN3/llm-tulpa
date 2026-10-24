//! What the host hands a plugin to work with. A plugin does not reach for the database or the agent itself, and it is not
//! given anything by name in `bootstrap`: the registry builds its `PluginEnv` from the `PluginHost` when it registers it,
//! and passes that to `PluginBuilder::build`. A plugin takes what it uses and ignores the rest; a new thing the host can
//! offer is a new field here, and no plugin changes for it.

use std::path::PathBuf;
use std::sync::Arc;

use sea_orm::DatabaseConnection;

use super::base::PluginError;
use super::schema;
use crate::facade::agent::Agent;
use crate::services::chat_store::ChatStore;

/// Everything the host can give a plugin, before it is narrowed to one plugin.
#[derive(Clone)]
pub struct PluginHost {
    pub db: DatabaseConnection,
    /// The agent the plugins run their own turns with (it takes no plugin prompt sections)
    pub agent: Arc<Agent>,
    pub chat_store: Arc<ChatStore>,
    /// The folder every plugin's own folder is inside, from `plugins_dir` in settings.json
    pub plugins_dir: PathBuf,
}

impl PluginHost {
    /// The environment of the plugin `(plugin_name, plugin_subname)`. Both names are held to the rule table prefixes
    /// follow (lowercase letters, digits, underscores), so neither can make the folder leave `plugins_dir`.
    pub fn env_for(&self, plugin_name: &str, plugin_subname: &str) -> Result<PluginEnv, PluginError> {
        schema::prefix(plugin_name, plugin_subname).map_err(|e| PluginError::Schema(e.to_string()))?;
        Ok(PluginEnv {
            db: self.db.clone(),
            agent: self.agent.clone(),
            chat_store: self.chat_store.clone(),
            data_dir: self.plugins_dir.join(plugin_name).join(plugin_subname),
        })
    }
}

/// What one plugin is given.
#[derive(Clone)]
pub struct PluginEnv {
    pub db: DatabaseConnection,
    pub agent: Arc<Agent>,
    pub chat_store: Arc<ChatStore>,
    /// `<plugins_dir>/<plugin_name>/<plugin_subname>`: the plugin's own place for files, and everything inside is its own
    /// business (how it lays it out, what it keeps per user). The folder is not created for the plugin; it makes what it
    /// needs when it first writes.
    pub data_dir: PathBuf,
}
