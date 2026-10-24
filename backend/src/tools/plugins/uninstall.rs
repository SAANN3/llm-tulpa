use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tool_derive::ToolParams;

use super::{approval, plugins_for_owner};
use crate::tools::base::{PropertyInfo, PropertyType, ResolvedScope, Tool, ToolContext, ToolError, ToolParams, ToolPermission, ToolSerializationError};

pub struct UninstallTool;

#[derive(Deserialize, ToolParams)]
struct UninstallArgs {
    #[tool(description = "The plugin, as plugins.list names it: name/subname.")]
    plugin: String,
    #[tool(description = "Also delete what the plugin kept: its settings, its database tables and its files. Leave it out unless the user said to delete that too.")]
    remove_data: Option<bool>,
}

fn split(plugin: &str) -> Result<(&str, &str), ToolError> {
    plugin.trim().split_once('/').ok_or_else(|| ToolError::FailedUnknown("the plugin is named name/subname, as plugins.list shows it".to_string()))
}

#[async_trait]
impl Tool for UninstallTool {
    fn function_name(&self) -> &str {
        "plugins.uninstall"
    }

    fn description(&self) -> &str {
        "Uninstalls an external plugin: its program is deleted from this machine. Do this only when the user has asked you to remove \
         that plugin; never on your own initiative, never to clean up, and never to retry a failed install. What the plugin kept \
         (its settings, tables and files) stays unless the user also said to delete it, which is remove_data. The user is asked to approve."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        UninstallArgs::tool_properties()
    }

    /// Asked about each plugin, and removing its data is a separate question from removing the plugin
    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        if let Some(refused) = super::refuse_non_owner(user_id) {
            return Ok(refused);
        }
        let args: UninstallArgs = serde_json::from_value(data)?;
        let plugin = args.plugin.trim().to_string();
        Ok(if args.remove_data == Some(true) {
            approval(
                "uninstall_with_data",
                &plugin,
                &scope,
                format!("uninstalling {plugin} with its data needs the owner's approval"),
                format!("Uninstall the plugin {plugin} and DELETE what it kept (its settings, its database tables and its files)? This cannot be undone."),
            )
        } else {
            approval(
                "uninstall",
                &plugin,
                &scope,
                format!("uninstalling {plugin} needs the owner's approval"),
                format!("Uninstall the plugin {plugin}? Its program is deleted; what it kept stays."),
            )
        })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: UninstallArgs = serde_json::from_value(data)?;
        let plugins = plugins_for_owner(ctx.user_id).await?;
        let (name, subname) = split(&args.plugin)?;
        let remove_data = args.remove_data == Some(true);
        plugins.uninstall(name, subname, remove_data).await.map_err(|e| ToolError::FailedUnknown(e.message()))?;
        Ok(json!({"uninstalled": args.plugin.trim(), "data_removed": remove_data}))
    }
}
