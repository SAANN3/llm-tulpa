//! Tools the model uses to connect a server itself. The settings tab does the same thing; these exist so a request
//! like "connect Firefox" can install the server, add it, and leave its tools for later turns.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::config::ConnectionConfig;
use super::runtime::Runtime;
use super::store::ServerStore;
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolPermission};

const ADD: &str = "Add an MCP server for this user and connect it now. Call this when the user asks to connect an \
external tool server, including one you have to install first (for example Firefox: install or start its MCP server, \
then add it here). After it connects, that server's tools show up under the name you chose, such as firefox.list_tabs, \
and stay available in later chats until removed or disabled. transport is \"http\" or \"stdio\". http needs url \
(and headers when the server asks for them, as a JSON object of name to value). stdio needs command, plus args, env \
and cwd when the program needs them. Install and configure the program yourself before this call; this tool only \
records the server and connects. A server name is lowercase letters, digits and underscores.";

const LIST: &str = "List the MCP servers this user already added, whether each is enabled, and how many tools the \
connected ones published. Check this before adding a server that might already be there.";

const REMOVE: &str = "Remove one of this user's MCP servers and its tools. The name is the one given to mcp.add_server.";

const ENABLE: &str = "Turn one of this user's MCP servers on or off. Off drops its tools until it is turned on again. \
The name is the one given to mcp.add_server.";

pub fn collect(store: Arc<ServerStore>, runtime: Arc<Runtime>) -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ListServers { store: store.clone(), runtime: runtime.clone() }),
        Box::new(AddServer { runtime: runtime.clone() }),
        Box::new(RemoveServer { runtime: runtime.clone() }),
        Box::new(SetEnabled { runtime }),
    ]
}

struct ListServers {
    store: Arc<ServerStore>,
    runtime: Arc<Runtime>,
}

#[async_trait]
impl Tool for ListServers {
    fn function_name(&self) -> &str { "mcp.list_servers" }
    fn description(&self) -> &str { LIST }
    fn required_properties(&self) -> Vec<PropertyInfo> { vec![] }

    async fn call_untyped(&self, _data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let rows = self.store.list(ctx.user_id).await.map_err(|e| ToolError::FailedUnknown(e.to_string()))?;
        let mut servers = Vec::new();
        for row in rows {
            let tools = self.runtime.tool_count(ctx.user_id, &row.name).await;
            servers.push(json!({
                "name": row.name,
                "transport": row.config.transport(),
                "target": row.config.target(),
                "enabled": row.enabled,
                "connected": tools.is_some(),
                "tools": tools.unwrap_or(0),
                "error": row.last_error,
            }));
        }
        Ok(json!({ "servers": servers }))
    }
}

struct AddServer {
    runtime: Arc<Runtime>,
}

#[derive(Deserialize)]
struct AddArgs {
    name: String,
    transport: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    cwd: Option<String>,
}

#[async_trait]
impl Tool for AddServer {
    fn function_name(&self) -> &str { "mcp.add_server" }
    fn description(&self) -> &str { ADD }
    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![
            prop("name", "Server name, also the domain of its tools", true),
            prop("transport", "http or stdio", true),
            prop("url", "URL of an http server", false),
            prop_object("headers", "HTTP headers, name to value, when the server asks for them"),
            prop("command", "Program to start for a stdio server", false),
            prop("args", "Arguments for a stdio server", false),
            prop_object("env", "Extra environment variables for a stdio server, name to value"),
            prop("cwd", "Working directory for a stdio server", false),
        ]
    }

    fn is_dangerous(&self, _data: Value, _scope: crate::tools::base::ResolvedScope) -> Result<ToolPermission, crate::tools::base::ToolSerializationError> {
        // Starting a program, or sending the user's headers to a URL, is the user's decision.
        Ok(ToolPermission::Denied { reason: "connecting an MCP server".into(), escalation: None })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: AddArgs = serde_json::from_value(data)?;
        let config = match args.transport.as_str() {
            "http" => ConnectionConfig::Http {
                url: args.url.ok_or_else(|| ToolError::FailedUnknown("http needs a url".into()))?,
                headers: string_map(args.headers)?,
            },
            "stdio" => ConnectionConfig::Stdio {
                command: args.command.ok_or_else(|| ToolError::FailedUnknown("stdio needs a command".into()))?,
                args: args.args,
                env: string_map(args.env)?,
                cwd: args.cwd,
            },
            other => return Err(ToolError::FailedUnknown(format!("transport is http or stdio, not '{other}'"))),
        };
        let count = self.runtime.add(ctx.user_id, &args.name, config).await.map_err(ToolError::FailedUnknown)?;
        Ok(json!({ "name": args.name, "tools": count }))
    }
}

struct RemoveServer {
    runtime: Arc<Runtime>,
}

#[async_trait]
impl Tool for RemoveServer {
    fn function_name(&self) -> &str { "mcp.remove_server" }
    fn description(&self) -> &str { REMOVE }
    fn required_properties(&self) -> Vec<PropertyInfo> { vec![prop("name", "Server name", true)] }

    fn is_dangerous(&self, _data: Value, _scope: crate::tools::base::ResolvedScope) -> Result<ToolPermission, crate::tools::base::ToolSerializationError> {
        Ok(ToolPermission::Denied { reason: "removing an MCP server".into(), escalation: None })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let name = data.get("name").and_then(|v| v.as_str()).ok_or_else(|| ToolError::FailedUnknown("name is required".into()))?;
        self.runtime.remove(ctx.user_id, name).await.map_err(ToolError::FailedUnknown)?;
        Ok(json!({ "removed": name }))
    }
}

struct SetEnabled {
    runtime: Arc<Runtime>,
}

#[async_trait]
impl Tool for SetEnabled {
    fn function_name(&self) -> &str { "mcp.set_enabled" }
    fn description(&self) -> &str { ENABLE }
    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![prop("name", "Server name", true), prop("enabled", "true to connect, false to disconnect", true)]
    }

    fn is_dangerous(&self, _data: Value, _scope: crate::tools::base::ResolvedScope) -> Result<ToolPermission, crate::tools::base::ToolSerializationError> {
        Ok(ToolPermission::Denied { reason: "changing an MCP server".into(), escalation: None })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let name = data.get("name").and_then(|v| v.as_str()).ok_or_else(|| ToolError::FailedUnknown("name is required".into()))?;
        let enabled = data.get("enabled").and_then(|v| v.as_bool()).ok_or_else(|| ToolError::FailedUnknown("enabled is required".into()))?;
        self.runtime.set_enabled(ctx.user_id, name, enabled).await.map_err(ToolError::FailedUnknown)?;
        Ok(json!({ "name": name, "enabled": enabled }))
    }
}

fn prop_object(name: &str, description: &str) -> PropertyInfo {
    PropertyInfo {
        name: name.into(),
        property_type: PropertyType::Object { properties: vec![] },
        description: description.into(),
        required: false,
    }
}

fn prop(name: &str, description: &str, required: bool) -> PropertyInfo {
    PropertyInfo {
        name: name.into(),
        property_type: if name == "args" {
            PropertyType::Array { items: Box::new(PropertyType::String) }
        } else if name == "enabled" {
            PropertyType::Boolean
        } else {
            PropertyType::String
        },
        description: description.into(),
        required,
    }
}

fn string_map(map: Option<serde_json::Map<String, Value>>) -> Result<std::collections::BTreeMap<String, String>, ToolError> {
    let Some(map) = map else { return Ok(std::collections::BTreeMap::new()) };
    map.into_iter()
        .map(|(k, v)| {
            let text = v.as_str().map(str::to_string).ok_or_else(|| ToolError::FailedUnknown(format!("'{k}' must be a string")))?;
            Ok((k, text))
        })
        .collect()
}
