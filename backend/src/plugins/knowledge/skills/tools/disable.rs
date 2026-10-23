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

pub struct DisableTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct DisableArgs {
    #[tool(description = "The name of a skill you wrote.")]
    name: String,
}

#[derive(Serialize)]
struct DisableOut {
    disabled: String,
    note: &'static str,
}

#[async_trait]
impl Tool for DisableTool {
    fn function_name(&self) -> &str {
        "skills.disable"
    }

    fn description(&self) -> &str {
        "Turns off a skill you wrote, for when it is outdated or wrong but the user may want it back: it leaves your list of \
         skills and you cannot read it, while its files are kept and the user can turn it on again in Settings. A skill the \
         user added cannot be turned off by you. To remove a skill for good, use skills.delete."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        DisableArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageWrite]
    }

    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let _: DisableArgs = serde_json::from_value(data)?;
        Ok(check_folder(&self.store, user_id, SharedBucket::StorageWrite, &scope))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: DisableArgs = serde_json::from_value(data)?;
        self.store.set_enabled(ctx.user_id, &args.name, false, Origin::Model).await.map_err(failed)?;
        Ok(serde_json::to_value(DisableOut {
            disabled: args.name,
            note: "turned off; it is out of your list from the next chat on and only the user can turn it back on",
        })?)
    }
}
