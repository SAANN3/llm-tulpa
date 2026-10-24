use async_trait::async_trait;
use serde_json::{Value, json};

use super::plugins_for_owner;
use crate::tools::base::{PropertyInfo, Tool, ToolContext, ToolError};

pub struct ListTool;

#[async_trait]
impl Tool for ListTool {
    fn function_name(&self) -> &str {
        "plugins.list"
    }

    fn description(&self) -> &str {
        "Lists the external plugins installed on this machine (separate programs that add tools and more), each with its version, \
         where it came from, whether it is running, and why not when it is not. Built-in plugins are not in this list."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let plugins = plugins_for_owner(ctx.user_id).await?;
        let list = plugins.list().await.map_err(|e| ToolError::FailedUnknown(e.message.unwrap_or_default()))?;
        Ok(Value::Array(
            list.into_iter()
                .map(|p| {
                    json!({
                        "plugin": format!("{}/{}", p.name, p.subname),
                        "version": p.version,
                        "source": p.source_type,
                        "source_url": p.source_url,
                        "state": p.state,
                        "last_error": p.last_error,
                        "process": p.process.map(|process| process.state),
                    })
                })
                .collect(),
        ))
    }
}
