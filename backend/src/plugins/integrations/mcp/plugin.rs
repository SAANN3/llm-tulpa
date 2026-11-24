use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use serde_json::{Value, json};

use super::runtime::Runtime;
use super::store::ServerStore;
use super::{tab, tools};
use crate::plugins::base::{Plugin, PluginError};
use crate::plugins::tab::{TabInfo, TabLayout};
use crate::tools::base::Tool;

pub struct McpPlugin {
    store: Arc<ServerStore>,
    runtime: Arc<Runtime>,
}

impl McpPlugin {
    pub fn new(store: Arc<ServerStore>, runtime: Arc<Runtime>) -> Self {
        Self { store, runtime }
    }
}

#[async_trait]
impl Plugin for McpPlugin {
    fn plugin_name(&self) -> &str { "integrations" }
    fn plugin_subname(&self) -> &str { "mcp" }
    fn settings_value(&self) -> Value { json!({}) }
    fn api_router(&self) -> Router { Router::new() }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        tools::collect(self.store.clone(), self.runtime.clone())
    }

    fn settings_tab(&self) -> Option<TabInfo> {
        Some(TabInfo { id: "mcp".into(), title: "MCP".into() })
    }

    async fn tab_layout(&self, user_id: i64) -> Result<TabLayout, PluginError> {
        tab::layout(&self.store, &self.runtime, user_id, None).await
    }

    async fn tab_action(&self, user_id: i64, action: &str, args: Value) -> Result<TabLayout, PluginError> {
        tab::action(&self.store, &self.runtime, user_id, action, args).await
    }

    async fn on_enabled(&self) -> Result<(), PluginError> {
        let runtime = self.runtime.clone();
        // Connecting can wait on a program or a URL; the plugin is already on, and each row records its own failure.
        tokio::spawn(async move { runtime.restore().await });
        Ok(())
    }

    async fn on_disabled(&self) -> Result<(), PluginError> {
        self.runtime.shutdown().await;
        Ok(())
    }

    async fn on_user_deleted(&self, user_id: i64) {
        self.runtime.forget_user(user_id).await;
    }

    /// Names of connected servers only. How to add or change one is in the tool definitions on this request, if those tools are sent.
    async fn prompt_section(&self, user_id: i64, tools: bool) -> Option<String> {
        if !tools {
            return None;
        }
        let servers = self.store.list(user_id).await.ok()?;
        let connected: Vec<&str> = servers.iter().filter(|s| s.enabled).map(|s| s.name.as_str()).collect();
        let list = if connected.is_empty() { "none".to_string() } else { connected.join(", ") };
        Some(format!(
            "MCP servers for this user: {list}. When the user asks to connect an external tool server, install or start it yourself, then check whether a tool in this request can add it (http or stdio). Its tools then stay available. Check whether a tool in this request can list the connected servers, remove one, or enable or disable one."
        ))
    }
}
