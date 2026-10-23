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

pub struct DeleteTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct DeleteArgs {
    #[tool(description = "The name of a skill you wrote.")]
    name: String,
}

#[derive(Serialize)]
struct DeleteOut {
    deleted: String,
}

#[async_trait]
impl Tool for DeleteTool {
    fn function_name(&self) -> &str {
        "skills.delete"
    }

    fn description(&self) -> &str {
        "Deletes a skill you wrote, with all its files. A skill the user added cannot be deleted by you."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        DeleteArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageDelete]
    }

    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let _: DeleteArgs = serde_json::from_value(data)?;
        Ok(check_folder(&self.store, user_id, SharedBucket::StorageDelete, &scope))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: DeleteArgs = serde_json::from_value(data)?;
        self.store.delete(ctx.user_id, &args.name, Origin::Model).await.map_err(failed)?;
        Ok(serde_json::to_value(DeleteOut { deleted: args.name })?)
    }
}
