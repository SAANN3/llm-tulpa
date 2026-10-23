use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::{check_folder, failed};
use crate::plugins::knowledge::skills::store::{Origin, SkillStore};
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

pub struct AddFileTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct AddFileArgs {
    #[tool(description = "The name of a skill you wrote.")]
    name: String,
    #[tool(description = "Where the file goes inside the skill, for example scripts/run.sh or notes/format.md. Not SKILL.md.")]
    path: String,
    #[tool(description = "The file's text.")]
    content: String,
}

#[derive(Serialize)]
struct AddFileOut {
    name: String,
    path: String,
}

#[async_trait]
impl Tool for AddFileTool {
    fn function_name(&self) -> &str {
        "skills.add_file"
    }

    fn description(&self) -> &str {
        "Adds a text file to a skill you wrote (a script, a template, notes the instructions refer to), or replaces the file of \
         that path. The skill's instructions are changed with skills.write, not here."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        AddFileArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageWrite]
    }

    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let _: AddFileArgs = serde_json::from_value(data)?;
        Ok(check_folder(&self.store, user_id, SharedBucket::StorageWrite, &scope))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: AddFileArgs = serde_json::from_value(data)?;
        self.store
            .add_file(ctx.user_id, &args.name, &args.path, args.content.into_bytes(), Origin::Model)
            .await
            .map_err(failed)?;
        Ok(serde_json::to_value(AddFileOut { name: args.name, path: args.path })?)
    }
}
