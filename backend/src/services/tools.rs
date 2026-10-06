use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use serde_json::Value;

use crate::{
    services::error::ErrorService,
    tools::base::{Tool, ToolContext, ToolError},
};

/// Owns every tool the agent can call, keyed by `Tool::function_name()`.
///
/// Static tools (registered at startup) live alongside plugin-provided tools
/// (registered/unregistered at runtime) in one flat map — this service tracks no
/// notion of "which plugin owns which name" itself: a plugin's own `tools()` already
/// lists its names on demand, so whoever is enabling/disabling one (`PluginRegistry`)
/// just calls it again and hands the names straight to `remove_tools`, the same way
/// `add_plugin_tools` is handed the tools to add.
///
/// Tools are stored as `Arc<dyn Tool>` (rather than `Box`) so a caller can take
/// an `Arc` clone and hold it past an `.await` point without keeping the lock open —
/// critical for agent turns that call `.is_dangerous`/`.call_untyped` well after
/// the initial lookup.
pub struct ToolService {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolService {
    /// Panics if two static tools share a `function_name()`. A name collision means
    /// the model can't tell the tools apart either, so this is a startup-time
    /// configuration mistake, not something worth handling gracefully at runtime.
    pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
        let mut map = HashMap::with_capacity(tools.len());
        for tool in tools {
            let name = tool.function_name().to_string();
            let arc: Arc<dyn Tool> = Arc::from(tool);
            if map.insert(name.clone(), arc).is_some() {
                panic!("Tool collision: multiple static tools registered with name '{name}'");
            }
        }
        Self {
            tools: RwLock::new(map),
        }
    }

    /// Registers a plugin's tools.
    ///
    /// Validates that **none** of the incoming tools collide with already-registered
    /// tools before inserting **any** of them — a partial-insert state is never
    /// possible. Returns `Err` (without modifying anything) if any collision is found,
    /// including duplicates within the supplied batch.
    pub async fn add_plugin_tools(&self, tools: Vec<Box<dyn Tool>>) -> Result<(), ToolServiceError> {
        if tools.is_empty() {
            return Ok(());
        }

        // --- Phase 1: validate the whole batch before touching the map ---
        {
            let map = self.tools.read().await;
            // Check for duplicates within the batch itself first
            let mut incoming: HashMap<&str, ()> = HashMap::with_capacity(tools.len());
            for tool in &tools {
                let name = tool.function_name();
                if incoming.insert(name, ()).is_some() {
                    return Err(ToolServiceError::Collision(name.to_string()));
                }
                if map.contains_key(name) {
                    return Err(ToolServiceError::Collision(name.to_string()));
                }
            }
        }

        // --- Phase 2: insert (all or nothing, since phase 1 passed) ---
        let mut map = self.tools.write().await;
        for tool in tools {
            let name = tool.function_name().to_string();
            map.insert(name, Arc::from(tool));
        }
        Ok(())
    }

    /// Removes tools by name — the caller (a plugin's own `tools()`, called again at
    /// the point of disabling it) already knows exactly which ones to name; this
    /// service doesn't track plugin ownership itself. A no-op for any name not present.
    pub async fn remove_tools(&self, names: &[String]) {
        if names.is_empty() {
            return;
        }
        let mut map = self.tools.write().await;
        for name in names {
            map.remove(name.as_str());
        }
    }

    /// Returns an `Arc` clone of a tool by name, or `None`.
    ///
    /// The `Arc` lets callers hold the reference past an `.await` point without
    /// keeping the lock open — critical for permission checks and tool execution
    /// in agent turns.
    pub async fn get_tool(&self, function_name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().await.get(function_name).cloned()
    }

    /// Looks up a tool by name and runs it. A missing name surfaces as
    /// `ToolError::FailedUnknown` — the model hallucinated a tool name.
    pub async fn call_tool(&self, function_name: &str, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let tool = self.tools.read().await.get(function_name).cloned().ok_or_else(|| {
            ToolError::FailedUnknown(format!("no tool named '{function_name}'"))
        })?;
        tool.call_untyped(data, ctx).await
    }

    /// Returns a snapshot of all registered tools as `Arc<dyn Tool>` pointers.
    /// Callers can hold these past `.await` points without keeping the lock open.
    /// Used by `Turn` to build the tool schema for every model request.
    pub async fn snapshot_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.read().await.values().cloned().collect()
    }
}

#[derive(Debug)]
pub enum ToolServiceError {
    /// A tool name from the plugin batch already exists in the map (or appears twice
    /// within the batch itself).
    Collision(String),
}

impl std::fmt::Display for ToolServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolServiceError::Collision(name) => {
                write!(f, "tool name '{name}' is already registered")
            }
        }
    }
}

impl From<ToolError> for ErrorService {
    fn from(err: ToolError) -> Self {
        ErrorService::internal(err.to_string())
    }
}
