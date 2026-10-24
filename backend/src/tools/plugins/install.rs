use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tool_derive::ToolParams;

use super::{approval, plugins_for_owner, staged_tree};
use crate::plugins::external::installer::TaskState;
use crate::plugins::external::stage::manifest_of;
use crate::tools::base::{PropertyInfo, PropertyType, ResolvedScope, Tool, ToolContext, ToolError, ToolParams, ToolPermission, ToolSerializationError};

/// How long a call waits for an install: a build is allowed twenty minutes, and the call goes on a little beyond that
const MAX_WAIT: Duration = Duration::from_secs(22 * 60);
const POLL: Duration = Duration::from_millis(500);

pub struct InstallTool;

#[derive(Deserialize, ToolParams)]
struct InstallArgs {
    #[tool(description = "The id plugins.stage gave.")]
    id: String,
}

#[async_trait]
impl Tool for InstallTool {
    fn function_name(&self) -> &str {
        "plugins.install"
    }

    fn description(&self) -> &str {
        "Installs a plugin that plugins.stage fetched: builds it if it needs building, tries it, and puts it in place. Install a plugin \
         only when the user asked you to install it (they named it or gave its address); never on your own initiative, and never \
         because a plugin would help with a task. Before calling it, stage the plugin, read its files and tell the user what it is and \
         what it will run. The user is asked to approve the install. A new plugin is installed switched off: the user switches it on in \
         Plugins, after giving it its settings if it has any."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        InstallArgs::tool_properties()
    }

    /// Asked about every plugin, with what the owner needs to decide: what it is, where it came from and what will run. An
    /// approval is for this one staged plugin (its id is random), so allowing one never covers another.
    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        if let Some(refused) = super::refuse_non_owner(user_id) {
            return Ok(refused);
        }
        let args: InstallArgs = serde_json::from_value(data)?;
        let what = match staged_tree(&args.id).and_then(|tree| manifest_of(&tree).ok()) {
            Some((manifest, _)) => format!(
                "Install the plugin {}/{} v{}? It will run as a program on this machine with your account's rights. Runs: {}{}",
                manifest.name,
                manifest.subname,
                manifest.version,
                manifest.run_command().replace("{plugin_dir}", "<its folder>"),
                manifest.build.as_deref().map(|build| format!(". Builds with: {build}")).unwrap_or_default()
            ),
            None => format!("Install the staged plugin {}? It will run as a program on this machine with your account's rights.", args.id),
        };
        Ok(approval("install", &args.id, &scope, format!("installing a plugin ({}) needs the owner's approval", args.id), what))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: InstallArgs = serde_json::from_value(data)?;
        let plugins = plugins_for_owner(ctx.user_id).await?;
        let started = plugins.start_install(&args.id).await.map_err(|e| ToolError::FailedUnknown(e.message()))?;
        let waited = tokio::time::Instant::now();
        let task = loop {
            tokio::time::sleep(POLL).await;
            match plugins.task(&args.id) {
                Some(task) if task.state != TaskState::Running => break task,
                Some(_) if waited.elapsed() < MAX_WAIT => {}
                Some(task) => break task,
                None => return Err(ToolError::FailedUnknown("the install was lost".to_string())),
            }
        };
        let tail: Vec<&String> = task.log.iter().rev().take(25).rev().collect();
        match task.state {
            TaskState::Done => Ok(json!({
                "installed": format!("{}/{}", started.name, started.subname),
                "note": "It is installed and switched off unless it comes on by default. The user switches it on in Plugins, after giving it its settings if it has any.",
                "log": tail,
            })),
            TaskState::Failed => Err(ToolError::FailedUnknown(format!("the install failed: {}\nthe end of its log:\n{}", task.error.unwrap_or_default(), tail.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n")))),
            TaskState::Running => Err(ToolError::FailedUnknown("the install is still running after 22 minutes; look at it in Plugins".to_string())),
        }
    }
}
