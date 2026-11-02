use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use serde_json::Value;

use super::plugin::McpPlugin;
use super::runtime::Runtime;
use super::store::{self, ServerStore};
use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::plugins::env::PluginEnv;
use crate::tools::base::PropertyInfo;

/// One store for the life of the process. Each enabled instance gets its own runtime, dropped in `on_disabled`.
#[derive(Default)]
pub struct McpBuilder {
    store: OnceLock<Arc<ServerStore>>,
}

impl McpBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PluginBuilder for McpBuilder {
    fn plugin_name(&self) -> &str { "integrations" }
    fn plugin_subname(&self) -> &str { "mcp" }
    fn settings_schema(&self) -> Vec<PropertyInfo> { vec![] }

    fn summary(&self) -> &str {
        "MCP servers this user connected, and the tools those servers publish"
    }

    fn help_message(&self) -> String {
        "Add a server from this tab, or ask the model to connect one: it installs what it needs and calls mcp.add_server. \
         A server is either a URL (http) or a local program (stdio). Its tools are named after the server, such as firefox.list_tabs, \
         and only this user sees them."
            .into()
    }

    fn default_enabled(&self) -> bool { true }

    fn migrations(&self, prefix: &str) -> Vec<String> {
        store::migrations(prefix)
    }

    async fn build(&self, _settings: Value, env: &PluginEnv) -> Result<Arc<dyn Plugin>, PluginError> {
        let store = self.store.get_or_init(|| Arc::new(ServerStore::new(env.db.clone()))).clone();
        let runtime = Arc::new(Runtime::new(store.clone(), env.tools.clone()));
        Ok(Arc::new(McpPlugin::new(store, runtime)))
    }
}
