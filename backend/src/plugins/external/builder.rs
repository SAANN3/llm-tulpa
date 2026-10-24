//! The registry's view of an external plugin. `ExternalPluginBuilder` answers the static questions from the plugin's
//! manifest, and `ExternalPlugin` hands the registry proxy tools whose calls go to the process.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use serde_json::{Value, json};

use super::process::PluginProcess;
use super::rpc::{CallError, DEFAULT_CALL_TIMEOUT};
use super::manifest::Manifest;
use super::spec::ToolSpec;
use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::plugins::env::PluginEnv;
use crate::tools::base::{PropertyInfo, ResolvedScope, ScopeGrant, Tool, ToolContext, ToolError, ToolPermission, ToolSerializationError};

/// Lives as long as the backend, like every builder; it owns the process, so a settings rebuild does not restart it.
pub struct ExternalPluginBuilder {
    name: String,
    subname: String,
    summary: String,
    help_message: String,
    default_enabled: bool,
    tools: Arc<Vec<ToolSpec>>,
    process: Arc<PluginProcess>,
}

impl ExternalPluginBuilder {
    pub fn new(manifest: Manifest, tools: Vec<ToolSpec>, process: Arc<PluginProcess>) -> Self {
        Self {
            name: manifest.name,
            subname: manifest.subname,
            summary: manifest.summary,
            help_message: manifest.help_message,
            default_enabled: manifest.default_enabled,
            tools: Arc::new(tools),
            process,
        }
    }
}

#[async_trait]
impl PluginBuilder for ExternalPluginBuilder {
    fn plugin_name(&self) -> &str {
        &self.name
    }

    fn plugin_subname(&self) -> &str {
        &self.subname
    }

    fn settings_schema(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    fn summary(&self) -> &str {
        &self.summary
    }

    fn help_message(&self) -> String {
        self.help_message.clone()
    }

    fn default_enabled(&self) -> bool {
        self.default_enabled
    }

    async fn shutdown(&self) {
        self.process.shutdown().await;
    }

    async fn build(&self, _settings: Value, _env: &PluginEnv) -> Result<Arc<dyn Plugin>, PluginError> {
        Ok(Arc::new(ExternalPlugin {
            name: self.name.clone(),
            subname: self.subname.clone(),
            tools: self.tools.clone(),
            process: self.process.clone(),
        }))
    }
}

pub struct ExternalPlugin {
    name: String,
    subname: String,
    tools: Arc<Vec<ToolSpec>>,
    process: Arc<PluginProcess>,
}

#[async_trait]
impl Plugin for ExternalPlugin {
    fn plugin_name(&self) -> &str {
        &self.name
    }

    fn plugin_subname(&self) -> &str {
        &self.subname
    }

    fn settings_value(&self) -> Value {
        json!({})
    }

    fn api_router(&self) -> Router {
        Router::new()
    }

    async fn on_enabled(&self) -> Result<(), PluginError> {
        Ok(())
    }

    async fn on_disabled(&self) -> Result<(), PluginError> {
        Ok(())
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        self.tools
            .iter()
            .map(|spec| Box::new(ExternalTool { spec: spec.clone(), plugin: format!("{}/{}", self.name, self.subname), process: self.process.clone() }) as Box<dyn Tool>)
            .collect()
    }
}

/// A tool of an external plugin: the model sees it like any other, and running it is a `call_tool` to the process
struct ExternalTool {
    spec: ToolSpec,
    plugin: String,
    process: Arc<PluginProcess>,
}

#[async_trait]
impl Tool for ExternalTool {
    fn function_name(&self) -> &str {
        &self.spec.name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        self.spec.parameters.iter().map(|p| p.property_info()).collect()
    }

    /// A tool that asked for it (`confirm`) needs the user's approval before it first runs in a chat, and the approval holds for
    /// that chat like any built-in tool's. Nothing finer is offered: only the plugin knows what a call does, so the backend
    /// has no folder or host to scope a grant to.
    fn is_dangerous(&self, _data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let approved = scope.own.as_ref().and_then(|own| own.get("approved")).and_then(Value::as_bool) == Some(true);
        if !self.spec.confirm || approved {
            return Ok(ToolPermission::Allowed);
        }
        Ok(ToolPermission::Denied {
            reason: format!("{} (a tool of the external plugin {}) requires approval", self.spec.name, self.plugin),
            escalation: Some(ScopeGrant {
                scope: ResolvedScope { own: Some(json!({"approved": true})), shared: HashMap::new() },
                ui_message: format!("Allow the external plugin {} to run its tool '{}' for the rest of this chat?", self.plugin, self.spec.name),
            }),
        })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let params = json!({
            "tool": self.spec.name,
            "args": data,
            "ctx": {"user_id": ctx.user_id, "chat_id": ctx.chat_id, "model": ctx.model, "provider": ctx.provider},
        });
        let timeout = self.spec.timeout_s.map(Duration::from_secs).unwrap_or(DEFAULT_CALL_TIMEOUT);
        self.process.call("call_tool", params, timeout).await.map_err(|e| {
            ToolError::FailedUnknown(match e {
                CallError::Rpc(e) => e.message,
                CallError::Timeout => format!("the plugin {} did not answer within {} s", self.plugin, timeout.as_secs()),
                CallError::Closed => format!("the plugin {} is not running", self.plugin),
            })
        })
    }
}
