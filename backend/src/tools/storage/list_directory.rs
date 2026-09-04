use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

use super::{check_directory_scope, human_size, normalize};

pub struct ListDirectoryTool;

#[derive(Deserialize, ToolParams)]
struct ListDirectoryArgs {
    #[tool(
        description = "Absolute or relative path to the directory to list. Lists only \
                        its immediate contents, not subdirectories' contents."
    )]
    path: String,
}

#[derive(Serialize)]
struct EntryOut {
    name: String,
    is_dir: bool,
    size: String,
    modified: Option<String>,
    readonly: bool,
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn function_name(&self) -> &str {
        "storage.list_directory"
    }

    fn description(&self) -> &str {
        "Lists the immediate contents of a directory (like `ls -lsh`): each entry's \
         name, whether it's a directory, its size, when it was last modified, and \
         whether it's read-only."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ListDirectoryArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: ListDirectoryArgs = serde_json::from_value(data)?;
        Ok(check_directory_scope(
            &args.path,
            SharedBucket::StorageRead,
            scope.shared.get(&SharedBucket::StorageRead),
        ))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: ListDirectoryArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let mut read_dir = tokio::fs::read_dir(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't list '{}': {e}", path.display())))?;

        let mut entries = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't list '{}': {e}", path.display())))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| ToolError::FailedUnknown(format!("couldn't read metadata: {e}")))?;

            entries.push(EntryOut {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: human_size(metadata.len()),
                modified: metadata
                    .modified()
                    .ok()
                    .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339()),
                readonly: metadata.permissions().readonly(),
            });
        }

        Ok(serde_json::to_value(entries)?)
    }
}
