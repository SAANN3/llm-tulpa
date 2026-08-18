use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams, ToolPermission, ToolSerializationError};

use super::{check_file_scope, normalize};

pub struct WriteFileTool;

#[derive(Deserialize, ToolParams)]
struct WriteFileArgs {
    #[tool(
        description = "Absolute or relative path to the file to write. The containing \
                        directory must already exist (use storage.create_directory \
                        first if not)."
    )]
    path: String,
    #[tool(
        description = "The text to add when `append` is true, or the file's entire new \
                        content when `append` is omitted/false."
    )]
    content: String,
    #[tool(
        description = "`true` adds `content` to the end of the file, creating it if it \
                        doesn't exist yet. Omitted or `false` overwrites the entire file \
                        with `content`, also creating it if needed."
    )]
    append: Option<bool>,
}

#[derive(Serialize)]
struct WriteFileOut {
    path: String,
    mode: &'static str,
    bytes_written: usize,
}

#[async_trait]
impl Tool for WriteFileTool {
    fn function_name(&self) -> &str {
        "storage.write_file"
    }

    fn description(&self) -> &str {
        "Writes to a file — either the whole thing (creating it if needed) or appending to \
         the end. Reserve this for a genuinely new file or a real full rewrite. To change \
         part of a file that already exists, use storage.replace_str instead — it's faster \
         and can't accidentally drop something elsewhere in the file the way reconstructing \
         the whole thing from memory can."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        WriteFileArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: Option<Value>,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: WriteFileArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, scope))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: WriteFileArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let (bytes_written, mode) = if args.append.unwrap_or(false) {
            append_to_file(&path, &args.content).await?
        } else {
            tokio::fs::write(&path, &args.content)
                .await
                .map_err(|e| ToolError::FailedUnknown(format!("couldn't write '{}': {e}", path.display())))?;
            (args.content.len(), "overwritten")
        };

        Ok(serde_json::to_value(WriteFileOut {
            path: path.to_string_lossy().to_string(),
            mode,
            bytes_written,
        })?)
    }
}

async fn append_to_file(path: &std::path::Path, content: &str) -> Result<(usize, &'static str), ToolError> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't open '{}': {e}", path.display())))?;

    file.write_all(content.as_bytes())
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't write '{}': {e}", path.display())))?;

    Ok((content.len(), "appended"))
}
