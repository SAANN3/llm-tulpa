use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams, ToolPermission, ToolSerializationError};

use super::{check_directory_scope, normalize};

pub struct CreateDirectoryTool;

#[derive(Deserialize, ToolParams)]
struct CreateDirectoryArgs {
    #[tool(
        description = "Absolute or relative path to the directory to create. Missing \
                        parent directories are created too, and it's not an error if \
                        the directory already exists."
    )]
    path: String,
}

#[derive(Serialize)]
struct CreateDirectoryOut {
    path: String,
    created: bool,
}

#[async_trait]
impl Tool for CreateDirectoryTool {
    fn function_name(&self) -> &str {
        "storage.create_directory"
    }

    fn description(&self) -> &str {
        "Creates a directory, including any missing parent directories (like `mkdir \
         -p`)."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        CreateDirectoryArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: Option<Value>,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: CreateDirectoryArgs = serde_json::from_value(data)?;
        Ok(check_directory_scope(&args.path, scope))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: CreateDirectoryArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't create '{}': {e}", path.display())))?;

        Ok(serde_json::to_value(CreateDirectoryOut {
            path: path.to_string_lossy().to_string(),
            created: true,
        })?)
    }
}
