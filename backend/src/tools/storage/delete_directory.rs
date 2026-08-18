use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams, ToolPermission, ToolSerializationError};

use super::{check_directory_scope, normalize};

pub struct DeleteDirectoryTool;

#[derive(Deserialize, ToolParams)]
struct DeleteDirectoryArgs {
    #[tool(
        description = "Absolute or relative path to the directory to delete, along \
                        with everything inside it (like `rm -rf`)."
    )]
    path: String,
}

#[derive(Serialize)]
struct DeleteDirectoryOut {
    path: String,
    deleted: bool,
}

#[async_trait]
impl Tool for DeleteDirectoryTool {
    fn function_name(&self) -> &str {
        "storage.delete_directory"
    }

    fn description(&self) -> &str {
        "Deletes a directory and everything inside it, recursively (like `rm -rf`). \
         Fails if the path is a file — use storage.delete_file for that."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        DeleteDirectoryArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: Option<Value>,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: DeleteDirectoryArgs = serde_json::from_value(data)?;
        Ok(check_directory_scope(&args.path, scope))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: DeleteDirectoryArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        tokio::fs::remove_dir_all(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't delete '{}': {e}", path.display())))?;

        Ok(serde_json::to_value(DeleteDirectoryOut {
            path: path.to_string_lossy().to_string(),
            deleted: true,
        })?)
    }
}
