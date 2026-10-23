use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::{check_folder, failed};
use crate::plugins::knowledge::skills::format;
use crate::plugins::knowledge::skills::pack::SKILL_FILE;
use crate::plugins::knowledge::skills::store::{Origin, SkillStore};
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

pub struct WriteTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct WriteArgs {
    #[tool(description = "The skill's name: lowercase letters and digits, in words joined by hyphens (for example lab-report).")]
    name: String,
    #[tool(
        description = "One or two sentences on when to use the skill. This is all that is shown in your instructions, so \
                        it has to be enough to tell whether a task matches."
    )]
    description: String,
    #[tool(description = "The skill's instructions, in markdown, without a header: the steps to follow and what to watch for.")]
    content: String,
}

#[derive(Serialize)]
struct WriteOut {
    name: String,
    /// What to do next, said in the result itself where it is read more reliably than in the description
    note: &'static str,
}

#[async_trait]
impl Tool for WriteTool {
    fn function_name(&self) -> &str {
        "skills.write"
    }

    fn description(&self) -> &str {
        "Saves a skill: instructions for a kind of task, to read the next time one comes up. Before writing, check skills.list \
         for a name already in use: writing a name you made earlier replaces that skill, and a skill the user added cannot \
         be changed, so it needs another name. Other files for the skill are added with skills.add_file."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        WriteArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageWrite]
    }

    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let _: WriteArgs = serde_json::from_value(data)?;
        Ok(check_folder(&self.store, user_id, SharedBucket::StorageWrite, &scope))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: WriteArgs = serde_json::from_value(data)?;
        let text = format::render(&args.name, &args.description, &args.content);
        self.store
            .save(ctx.user_id, &args.name, &args.description, vec![(SKILL_FILE.to_string(), text.into_bytes())], Origin::Model)
            .await
            .map_err(failed)?;
        Ok(serde_json::to_value(WriteOut {
            name: args.name,
            note: "saved; it shows in the list of skills from the next chat on, and skills.read opens it now",
        })?)
    }
}
