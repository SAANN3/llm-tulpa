use std::collections::HashMap;

use serde_json::Value;

use crate::{
    services::error::ErrorService,
    tools::base::{Tool, ToolError},
};

/// Owns every tool the agent can call, keyed by `Tool::function_name()`. Built once from
/// the concrete tool list and never mutated after — resolving a model-requested tool
/// name to an actual `Tool` impl always goes through here.
pub struct ToolService {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolService {
    /// Panics if two tools share a `function_name()`. A name collision means the model
    /// can't tell the tools apart either, so this is a startup-time configuration
    /// mistake, not something worth handling gracefully at runtime.
    pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
        let mut map = HashMap::with_capacity(tools.len());

        for tool in tools {
            let name = tool.function_name().to_string();
            if map.insert(name.clone(), tool).is_some() {
                panic!("Tool collision: multiple tools registered with name '{name}'");
            }
        }

        Self { tools: map }
    }

    /// Looks up a tool by name and runs it. A missing name surfaces as
    /// `ToolError::FailedUnknown` rather than panicking — unlike the collision above,
    /// this can happen at runtime any time the model hallucinates a tool name, so it's a
    /// normal error the caller is expected to handle.
    pub async fn call_tool(&self, function_name: &str, data: Value) -> Result<Value, ToolError> {
        let tool = self.get_tool(function_name).ok_or_else(|| {
            ToolError::FailedUnknown(format!("no tool named '{function_name}'"))
        })?;

        tool.call_untyped(data).await
    }

    pub fn get_tool(&self, function_name: &str) -> Option<&dyn Tool> {
        self.tools.get(function_name).map(|tool| tool.as_ref())
    }

    pub fn get_tools(&self) -> impl Iterator<Item = &Box<dyn Tool>> {
        self.tools.values()
    }
}

impl From<ToolError> for ErrorService {
    fn from(err: ToolError) -> Self {
        ErrorService::internal(err.to_string())
    }
}
