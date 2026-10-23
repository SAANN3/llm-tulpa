use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::{check_folder, failed};
use crate::plugins::knowledge::skills::pack;
use crate::plugins::knowledge::skills::store::{Origin, SkillStore};
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};
use crate::tools::storage::{check_directory_scope, check_file_scope, normalize};

pub struct InstallTool {
    pub store: Arc<SkillStore>,
}

#[derive(Deserialize, ToolParams)]
struct InstallArgs {
    #[tool(
        description = "Path of a folder that holds a SKILL.md (with the files it refers to), or of a .zip of one, on this machine."
    )]
    path: String,
}

#[derive(Serialize)]
struct InstallOut {
    name: String,
    description: String,
    files: usize,
}

#[async_trait]
impl Tool for InstallTool {
    fn function_name(&self) -> &str {
        "skills.install"
    }

    fn description(&self) -> &str {
        "Installs a skill from a folder or a .zip on this machine, for example one that was downloaded. The SKILL.md in it \
         names the skill and says when to use it. A skill the user added under the same name is not replaced."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        InstallArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead, SharedBucket::StorageWrite]
    }

    /// Reading the source and writing into the skills folder are two approvals, asked one at a time
    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: InstallArgs = serde_json::from_value(data)?;
        let read = scope.shared.get(&SharedBucket::StorageRead);
        // A folder is read whole, so the folder itself is what is asked about; for a file (a zip) it is the folder it is in
        let source = normalize(std::path::Path::new(&args.path));
        let permission = if source.is_dir() {
            check_directory_scope(&args.path, SharedBucket::StorageRead, read)
        } else {
            check_file_scope(&args.path, SharedBucket::StorageRead, read)
        };
        if let ToolPermission::Denied { .. } = permission {
            return Ok(permission);
        }
        Ok(check_folder(&self.store, user_id, SharedBucket::StorageWrite, &scope))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: InstallArgs = serde_json::from_value(data)?;
        let source: PathBuf = normalize(std::path::Path::new(&args.path));
        // Reading a folder or a zip is blocking work, kept off the async runtime
        let skill = tokio::task::spawn_blocking(move || pack::from_path(&source).and_then(pack::Pack::into_skill))
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("reading the skill failed: {e}")))?
            .map_err(ToolError::FailedUnknown)?;
        let files = skill.files.len();
        self.store
            .save(ctx.user_id, &skill.name, &skill.description, skill.files, Origin::Model)
            .await
            .map_err(failed)?;
        Ok(serde_json::to_value(InstallOut { name: skill.name, description: skill.description, files })?)
    }
}
