use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::{as_text, failed};
use crate::plugins::knowledge::skills::pack::SKILL_FILE;
use crate::plugins::knowledge::skills::store::SkillStore;
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct ReadTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct ReadArgs {
    #[tool(description = "The skill's name, as in the list of skills in your instructions.")]
    name: String,
}

#[derive(Serialize)]
struct ReadOut {
    name: String,
    /// The whole SKILL.md: its header and the instructions
    content: String,
    /// The skill's other files, to open with skills.read_file when the instructions point to them
    other_files: Vec<String>,
}

#[async_trait]
impl Tool for ReadTool {
    fn function_name(&self) -> &str {
        "skills.read"
    }

    fn description(&self) -> &str {
        "Reads a skill's instructions (its SKILL.md) and lists the skill's other files. Check the list of skills in your \
         instructions before starting a task; when a description matches the task, read that skill first and follow it. Its \
         other files are opened with skills.read_file, only when the instructions send you to them."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReadArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadArgs = serde_json::from_value(data)?;
        self.store.get_enabled(ctx.user_id, &args.name).await.map_err(failed)?;
        let bytes = self.store.read_file(ctx.user_id, &args.name, SKILL_FILE).await.map_err(failed)?;
        let content = as_text(SKILL_FILE, bytes)?;
        let other_files = self
            .store
            .files(ctx.user_id, &args.name)
            .await
            .map_err(failed)?
            .into_iter()
            .filter(|path| path != SKILL_FILE)
            .collect();
        Ok(serde_json::to_value(ReadOut { name: args.name, content, other_files })?)
    }
}
