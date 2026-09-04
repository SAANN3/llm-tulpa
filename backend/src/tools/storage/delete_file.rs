use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

use super::{check_file_scope, normalize};

pub struct DeleteFileTool;

#[derive(Deserialize, ToolParams)]
struct DeleteFileArgs {
    #[tool(description = "Absolute or relative path to the file to delete.")]
    path: String,
}

#[derive(Serialize)]
struct DeleteFileOut {
    path: String,
    deleted: bool,
}

#[async_trait]
impl Tool for DeleteFileTool {
    fn function_name(&self) -> &str {
        "storage.delete_file"
    }

    fn description(&self) -> &str {
        "Deletes a single file. Fails if the path is a directory — use \
         storage.delete_directory for that."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        DeleteFileArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageDelete]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: DeleteFileArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, SharedBucket::StorageDelete, scope.shared.get(&SharedBucket::StorageDelete)))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: DeleteFileArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't delete '{}': {e}", path.display())))?;

        Ok(serde_json::to_value(DeleteFileOut {
            path: path.to_string_lossy().to_string(),
            deleted: true,
        })?)
    }
}
