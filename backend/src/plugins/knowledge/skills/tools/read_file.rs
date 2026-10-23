use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::{as_text, failed};
use crate::plugins::knowledge::skills::store::SkillStore;
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct ReadFileTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct ReadFileArgs {
    #[tool(description = "The skill's name, as in the list of skills in your instructions.")]
    name: String,
    #[tool(description = "The file's path inside the skill, as skills.read listed it (for example scripts/run.sh).")]
    path: String,
}

#[derive(Serialize)]
struct ReadFileOut {
    path: String,
    content: String,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn function_name(&self) -> &str {
        "skills.read_file"
    }

    fn description(&self) -> &str {
        "Reads one text file of a skill, by the path skills.read listed. Only for a file the skill's instructions refer to."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReadFileArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        let bytes = self.store.read_file(ctx.user_id, &args.name, &args.path).await.map_err(failed)?;
        let content = as_text(&args.path, bytes)?;
        Ok(serde_json::to_value(ReadFileOut { path: args.path, content })?)
    }
}
