use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

use super::{check_file_scope, normalize};

pub struct ReadFileTool;

/// Hard ceiling on how much of a file `read_file` hands back in one call. Measured in
/// characters rather than tokens — counting real tokens needs the model's own
/// tokenizer, and characters are a close enough proxy without that dependency. A single
/// huge file (a bundled/minified asset, a log, a lockfile) could otherwise single-
/// handedly blow the model's context budget; this is generous enough that ordinary
/// source files still come back whole.
const MAX_READ_CHARS: usize = 40_000;

#[derive(Deserialize, ToolParams)]
struct ReadFileArgs {
    #[tool(description = "Absolute or relative path to the file to read.")]
    path: String,
}

#[derive(Serialize)]
struct ReadFileOut {
    content: String,
    /// `true` when `content` is only the first `MAX_READ_CHARS` characters of the real
    /// file — the model needs to know its view is partial, not just get silently fed
    /// less than what's actually there.
    truncated: bool,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn function_name(&self) -> &str {
        "storage.read_file"
    }

    fn description(&self) -> &str {
        "Reads a file's contents as text. Fails if the file isn't valid UTF-8 text. \
         Files longer than 40,000 characters come back truncated (see `truncated` in \
         the response) — the file itself is untouched, only what's returned here is cut \
         short."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReadFileArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, SharedBucket::StorageRead, scope.shared.get(&SharedBucket::StorageRead)))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;

        let total_chars = content.chars().count();
        let (content, truncated) = if total_chars > MAX_READ_CHARS {
            let cropped: String = content.chars().take(MAX_READ_CHARS).collect();
            (
                format!(
                    "{cropped}\n\n[... file truncated: showing the first {MAX_READ_CHARS} of {total_chars} characters ...]"
                ),
                true,
            )
        } else {
            (content, false)
        };

        Ok(serde_json::to_value(ReadFileOut { content, truncated })?)
    }
}
