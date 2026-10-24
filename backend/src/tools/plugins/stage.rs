use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tool_derive::ToolParams;

use super::plugins_for_owner;
use crate::plugins::external::installer::StagedPlugin;
use crate::plugins::external::stage::valid_git_url;
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams, ToolPermission, ToolSerializationError,
};
use crate::tools::storage::{check_directory_scope, check_file_scope, normalize};

pub struct StageTool;

#[derive(Deserialize, ToolParams)]
struct StageArgs {
    #[tool(
        description = "Where the plugin is: a git address (https://..., ssh://... or user@host:path), or the path of a plugin folder (with a plugin.toml in it) or of a .tulpa file on this machine."
    )]
    source: String,
    #[tool(description = "For a git address: the branch or tag to take; the repository's default branch when left out.")]
    git_ref: Option<String>,
}

/// What the model is told of a staged plugin: enough to judge it and to tell the user, without the whole listing
pub fn review(staged: &StagedPlugin) -> Value {
    let paths: Vec<&str> = staged.files.iter().take(80).map(|f| f.path.as_str()).collect();
    json!({
        "id": staged.id,
        "plugin": format!("{}/{}", staged.name, staged.subname),
        "version": staged.version,
        "action": staged.action,
        "replaces_version": staged.installed_version,
        "summary": staged.summary,
        "kind": staged.kind,
        "asks_for": {"settings": staged.settings, "a_tab": staged.has_tab, "a_database": staged.has_database},
        "commands": {"check": staged.check, "build": staged.build, "run": staged.run},
        "from": {"type": staged.source.source_type, "url": staged.source.url, "commit": staged.source.commit},
        "files": paths,
        "file_count": staged.files.len(),
        "readme": staged.readme.as_deref().map(|r| r.chars().take(2000).collect::<String>()),
        "problems": staged.problems,
        "next": if staged.problems.is_empty() {
            "Nothing is installed or run yet. Read what it does (plugins.read_file), tell the user what it is and what it will run, and install it with plugins.install only if the user asked for it."
        } else {
            "It cannot be installed as it is: see the problems."
        },
    })
}

#[async_trait]
impl Tool for StageTool {
    fn function_name(&self) -> &str {
        "plugins.stage"
    }

    fn description(&self) -> &str {
        "Fetches a plugin so that it can be looked at, without installing it or running anything of it: from a git address, or from a \
         folder or a .tulpa file on this machine. A plugin is a program that adds tools (and more) to this app, and it runs with the \
         owner's rights once installed, so look before you install. The answer says what the plugin is, what it asks for, the commands \
         it would run, and its files; read the ones that matter with plugins.read_file. Use it when the user wants to install a plugin, \
         never on your own initiative."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        StageArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    /// A git address is only fetched into the staging area, which needs no approval; a path on this machine is read, which is asked
    /// the way every read is
    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        if let Some(refused) = super::refuse_non_owner(user_id) {
            return Ok(refused);
        }
        let args: StageArgs = serde_json::from_value(data)?;
        if valid_git_url(args.source.trim()) {
            return Ok(ToolPermission::Allowed);
        }
        let granted = scope.shared.get(&SharedBucket::StorageRead);
        Ok(if normalize(std::path::Path::new(&args.source)).is_dir() {
            check_directory_scope(&args.source, SharedBucket::StorageRead, granted)
        } else {
            check_file_scope(&args.source, SharedBucket::StorageRead, granted)
        })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: StageArgs = serde_json::from_value(data)?;
        let plugins = plugins_for_owner(ctx.user_id).await?;
        let source = args.source.trim();
        let staged = if valid_git_url(source) {
            plugins.stage_git(source, args.git_ref.as_deref().map(str::trim).filter(|r| !r.is_empty())).await
        } else {
            plugins.stage_path(&normalize(std::path::Path::new(source))).await
        };
        let staged = staged.map_err(|e| ToolError::FailedUnknown(e.message()))?;
        Ok(review(&staged))
    }
}
