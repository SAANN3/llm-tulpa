use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use super::failed;
use crate::plugins::knowledge::skills::store::SkillStore;
use crate::tools::base::{PropertyInfo, Tool, ToolContext, ToolError};

pub struct ListTool {
    pub store: Arc<SkillStore>,
}

#[derive(Serialize)]
struct SkillOut {
    name: String,
    description: String,
    /// `user` for one the user added, `model` for one made through these tools
    origin: &'static str,
}

#[async_trait]
impl Tool for ListTool {
    fn function_name(&self) -> &str {
        "skills.list"
    }

    fn description(&self) -> &str {
        "Lists the user's skills, each with its description. The list in your instructions was made when this chat started, \
         so call this to see a skill that was added, written or deleted since."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let skills = self.store.list(ctx.user_id).await.map_err(failed)?;
        let out: Vec<SkillOut> = skills
            .into_iter()
            .map(|s| SkillOut { name: s.name, description: s.description, origin: s.origin.as_str() })
            .collect();
        Ok(serde_json::to_value(out)?)
    }
}
