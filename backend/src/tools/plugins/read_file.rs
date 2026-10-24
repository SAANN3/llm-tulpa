use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tool_derive::ToolParams;

use super::plugins_for_owner;
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct ReadFileTool;

#[derive(Deserialize, ToolParams)]
struct ReadFileArgs {
    #[tool(description = "The id plugins.stage gave.")]
    id: String,
    #[tool(description = "The file's path inside the plugin, as plugins.stage listed it (for example plugin.toml or main.py).")]
    path: String,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn function_name(&self) -> &str {
        "plugins.read_file"
    }

    fn description(&self) -> &str {
        "Reads one text file of a plugin that plugins.stage fetched, to see what it does before installing it. Read its plugin.toml \
         and its main program at least, and tell the user anything that looks wrong."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReadFileArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        let plugins = plugins_for_owner(ctx.user_id).await?;
        let content = plugins.staged_file(&args.id, &args.path).await.map_err(|e| ToolError::FailedUnknown(e.message()))?;
        Ok(json!({"path": args.path, "content": content}))
    }
}
