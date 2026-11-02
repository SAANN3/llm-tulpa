//! Live MCP clients. A server's tools are published only for the user who added it, and only while it is connected.
//! `replace_user` replaces the plugin's whole per-user list, so every change republishes the union of that user's servers.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ProtocolVersion};
use rmcp::service::{ClientLifecycleMode, ClientServiceExt, RunningService};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceError};
use serde_json::Value;
use tokio::sync::Mutex;

use super::config::ConnectionConfig;
use super::store::{Server, ServerStore, StoreError};
use crate::plugins::external::spec::valid_tool_name;
use crate::services::process::{command_env, detach, kill_process_tree};
use crate::services::tools::ToolControl;
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError};

struct Spec {
    local_name: String,
    remote_name: String,
    description: String,
    /// The server's input schema. Folded into `PropertyInfo` when the tool is advertised, so the shared tool type stays typed.
    schema: Value,
}

struct Session {
    client: RunningService<RoleClient, ()>,
    specs: Vec<Spec>,
    pid: Option<u32>,
}

pub struct Runtime {
    store: Arc<ServerStore>,
    tools: ToolControl,
    live: Arc<Mutex<HashMap<(i64, String), Session>>>,
}

impl Runtime {
    pub fn new(store: Arc<ServerStore>, tools: ToolControl) -> Self {
        Self { store, tools, live: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Connect every enabled server. Called when the plugin comes on; a failure stays on the row.
    pub async fn restore(&self) {
        let Ok(rows) = self.store.list_enabled().await else { return };
        let mut users = HashSet::new();
        for (user_id, server) in rows {
            users.insert(user_id);
            if let Err(e) = self.connect(user_id, &server).await {
                tracing::warn!("mcp server '{}' for user {user_id} did not connect: {e}", server.name);
            }
        }
        for user_id in users {
            self.publish(user_id).await;
        }
    }

    pub async fn add(&self, user_id: i64, name: &str, config: ConnectionConfig) -> Result<usize, String> {
        self.store.upsert(user_id, name, &config, true).await.map_err(|e| e.to_string())?;
        let server = Server { name: name.to_string(), config, enabled: true, last_error: None };
        let count = self.connect(user_id, &server).await?;
        self.publish(user_id).await;
        Ok(count)
    }

    pub async fn set_enabled(&self, user_id: i64, name: &str, enabled: bool) -> Result<(), String> {
        self.store.set_enabled(user_id, name, enabled).await.map_err(|e| e.to_string())?;
        if enabled {
            let server = self.store.list(user_id).await.map_err(|e| e.to_string())?.into_iter().find(|s| s.name == name);
            let Some(server) = server else { return Err(format!("no server named '{name}'")) };
            self.connect(user_id, &server).await?;
        } else {
            self.drop_session(user_id, name).await;
        }
        self.publish(user_id).await;
        Ok(())
    }

    pub async fn remove(&self, user_id: i64, name: &str) -> Result<(), String> {
        self.drop_session(user_id, name).await;
        self.store.remove(user_id, name).await.map_err(|e| e.to_string())?;
        self.publish(user_id).await;
        Ok(())
    }

    pub async fn tool_count(&self, user_id: i64, name: &str) -> Option<usize> {
        self.live.lock().await.get(&(user_id, name.to_string())).map(|s| s.specs.len())
    }

    /// Drop every client. The registry already clears this plugin's tools on disable.
    pub async fn shutdown(&self) {
        let sessions = std::mem::take(&mut *self.live.lock().await);
        for (_, session) in sessions {
            end_session(session).await;
        }
    }

    pub async fn forget_user(&self, user_id: i64) {
        let names: Vec<String> = self.live.lock().await.keys().filter(|(u, _)| *u == user_id).map(|(_, n)| n.clone()).collect();
        for name in names {
            self.drop_session(user_id, &name).await;
        }
    }

    async fn connect(&self, user_id: i64, server: &Server) -> Result<usize, String> {
        self.drop_session(user_id, &server.name).await;
        let (client, pid) = match open(&server.config).await {
            Ok(opened) => opened,
            Err(e) => {
                self.store.set_error(user_id, &server.name, Some(e.clone())).await.ok();
                return Err(e);
            }
        };
        let listed = match client.peer().list_all_tools().await {
            Ok(tools) => tools,
            Err(e) => {
                let message = e.to_string();
                end_session(Session { client, specs: vec![], pid }).await;
                self.store.set_error(user_id, &server.name, Some(message.clone())).await.ok();
                return Err(message);
            }
        };
        let specs = local_specs(&server.name, listed);
        let count = specs.len();
        self.store.set_error(user_id, &server.name, None).await.ok();
        self.live.lock().await.insert((user_id, server.name.clone()), Session { client, specs, pid });
        Ok(count)
    }

    async fn publish(&self, user_id: i64) {
        let specs: Vec<(String, Spec)> = {
            let live = self.live.lock().await;
            live.iter()
                .filter(|((user, _), _)| *user == user_id)
                .flat_map(|((_, name), session)| session.specs.iter().map(|spec| (name.clone(), clone_spec(spec))))
                .collect()
        };
        let sessions = self.live.clone();
        let tools: Vec<Box<dyn Tool>> = specs
            .into_iter()
            .map(|(server, spec)| Box::new(RemoteTool { server, spec, sessions: sessions.clone(), user_id }) as Box<dyn Tool>)
            .collect();
        if let Err(e) = self.tools.replace_user(user_id, tools).await {
            tracing::warn!("mcp tools for user {user_id} were not published: {e}");
        }
    }

    async fn drop_session(&self, user_id: i64, name: &str) {
        if let Some(session) = self.live.lock().await.remove(&(user_id, name.to_string())) {
            end_session(session).await;
        }
    }
}

fn clone_spec(spec: &Spec) -> Spec {
    Spec {
        local_name: spec.local_name.clone(),
        remote_name: spec.remote_name.clone(),
        description: spec.description.clone(),
        schema: spec.schema.clone(),
    }
}

async fn open(config: &ConnectionConfig) -> Result<(RunningService<RoleClient, ()>, Option<u32>), String> {
    let lifecycle = ClientLifecycleMode::Auto {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        legacy_version: Some(ProtocolVersion::V_2025_11_25),
    };
    match config {
        ConnectionConfig::Http { url, headers } => {
            let mut custom = HashMap::new();
            for (name, value) in headers {
                let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|e| e.to_string())?;
                let value = reqwest::header::HeaderValue::from_str(value).map_err(|e| e.to_string())?;
                custom.insert(name, value);
            }
            let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().map_err(|e| e.to_string())?;
            let transport = StreamableHttpClientTransport::with_client(
                client,
                StreamableHttpClientTransportConfig::with_uri(url.as_str()).custom_headers(custom),
            );
            let running = ().serve_with_lifecycle(transport, lifecycle).await.map_err(|e| e.to_string())?;
            Ok((running, None))
        }
        ConnectionConfig::Stdio { command, args, env, cwd } => {
            let mut child = tokio::process::Command::new(command);
            child.args(args).envs(command_env()).envs(env).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
            if let Some(dir) = cwd {
                child.current_dir(dir);
            }
            detach(&mut child);
            let transport = TokioChildProcess::new(child).map_err(|e| e.to_string())?;
            let pid = transport.id();
            let running = ().serve_with_lifecycle(transport, lifecycle).await.map_err(|e| e.to_string())?;
            Ok((running, pid))
        }
    }
}

async fn end_session(session: Session) {
    if let Some(pid) = session.pid {
        kill_process_tree(pid).await;
    }
    drop(session.client);
}

fn local_specs(server: &str, tools: Vec<rmcp::model::Tool>) -> Vec<Spec> {
    let mut used = HashSet::new();
    let mut specs = Vec::new();
    for tool in tools {
        let Some(local_name) = local_name(server, &tool.name, &mut used) else { continue };
        let description = tool.description.map(|d| d.into_owned()).unwrap_or_else(|| format!("Tool {local_name} from the {server} MCP server"));
        specs.push(Spec {
            local_name,
            remote_name: tool.name.into_owned(),
            description,
            schema: Value::Object(tool.input_schema.as_ref().clone()),
        });
    }
    specs
}

/// `enum`, `oneOf` and the other keywords `PropertyType` has no field for are dropped. A property whose type is none of the six becomes a string.
fn properties_from_schema(schema: &Value) -> Vec<PropertyInfo> {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else { return vec![] };
    let required: HashSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    properties
        .iter()
        .map(|(name, spec)| PropertyInfo {
            name: name.clone(),
            property_type: property_type(spec),
            description: spec.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
            required: required.contains(name.as_str()),
        })
        .collect()
}

fn property_type(spec: &Value) -> PropertyType {
    match spec.get("type").and_then(Value::as_str) {
        Some("number") => PropertyType::Number,
        Some("integer") => PropertyType::Integer,
        Some("boolean") => PropertyType::Boolean,
        Some("array") => PropertyType::Array { items: Box::new(spec.get("items").map(property_type).unwrap_or(PropertyType::String)) },
        Some("object") => PropertyType::Object { properties: properties_from_schema(spec) },
        _ => PropertyType::String,
    }
}

fn local_name(server: &str, remote: &str, used: &mut HashSet<String>) -> Option<String> {
    let mut action: String = remote
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    while action.contains("__") {
        action = action.replace("__", "_");
    }
    let action = action.trim_matches('_');
    if action.is_empty() {
        return None;
    }
    let action = if action.chars().next().is_some_and(|c| c.is_ascii_digit()) { format!("t_{action}") } else { action.to_string() };
    for n in 0..50 {
        let candidate = if n == 0 { format!("{server}.{action}") } else { format!("{server}.{action}_{n}") };
        if valid_tool_name(&candidate) && used.insert(candidate.clone()) {
            return Some(candidate);
        }
    }
    None
}

struct RemoteTool {
    server: String,
    spec: Spec,
    sessions: Arc<Mutex<HashMap<(i64, String), Session>>>,
    user_id: i64,
}

#[async_trait]
impl Tool for RemoteTool {
    fn function_name(&self) -> &str {
        &self.spec.local_name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        properties_from_schema(&self.spec.schema)
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        if ctx.user_id != self.user_id {
            return Err(ToolError::FailedUnknown("this MCP server belongs to another user".into()));
        }
        let arguments = match data {
            Value::Object(map) => Some(map),
            Value::Null => None,
            _ => return Err(ToolError::FailedUnknown("arguments must be an object".into())),
        };
        let live = self.sessions.lock().await;
        let Some(session) = live.get(&(self.user_id, self.server.clone())) else {
            return Err(ToolError::FailedUnknown(format!("server '{}' is not connected", self.server)));
        };
        let mut params = CallToolRequestParams::new(self.spec.remote_name.clone());
        if let Some(arguments) = arguments {
            params = params.with_arguments(arguments);
        }
        let result = session
            .client
            .peer()
            .call_tool(params)
            .await
            .map_err(|e: ServiceError| ToolError::FailedUnknown(e.to_string()))?;
        // The lock is held across the call so a reconnect cannot drop the client underneath it.
        drop(live);
        result_value(result)
    }
}

fn result_value(result: CallToolResult) -> Result<Value, ToolError> {
    let text = result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            ContentBlock::Image(image) => Some(format!("[image {}]", image.mime_type)),
            ContentBlock::Audio(audio) => Some(format!("[audio {}]", audio.mime_type)),
            ContentBlock::Resource(_) | ContentBlock::ResourceLink(_) => Some("[resource]".into()),
            _ => Some("[content]".into()),
        })
        .collect::<Vec<_>>()
        .join("\n");
    if result.is_error == Some(true) {
        return Err(ToolError::FailedUnknown(if text.is_empty() { "the MCP server reported an error".into() } else { text }));
    }
    if let Some(structured) = result.structured_content {
        return Ok(structured);
    }
    if result.content.len() == 1 && matches!(result.content.first(), Some(ContentBlock::Text(_))) {
        return Ok(Value::String(text));
    }
    Ok(Value::String(text))
}

impl From<StoreError> for String {
    fn from(e: StoreError) -> Self {
        e.to_string()
    }
}
