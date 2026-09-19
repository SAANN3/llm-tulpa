use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
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
    #[tool(description = "How many characters to skip from the start of the file before reading — to read past where an earlier call was cut off, pass the `next_offset` it returned. Defaults to 0 (start of the file).")]
    offset: Option<usize>,
}

#[derive(Serialize)]
struct ReadFileOut {
    content: String,
    /// `true` when there's more of the file after what `content` covers — the model
    /// needs to know its view is partial, not just get silently fed less than what's
    /// actually there.
    truncated: bool,
    /// The file's full length in characters, wherever this read started.
    total_chars: usize,
    /// Where to continue from (pass it back as `offset`) — only set when `truncated`.
    next_offset: Option<usize>,
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
         short; pass the response's `next_offset` as `offset` to read the next part."
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

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;

        let total_chars = content.chars().count();
        let offset = args.offset.unwrap_or(0);

        if offset >= total_chars && total_chars > 0 {
            return Ok(serde_json::to_value(ReadFileOut {
                content: format!("[offset {offset} is past the end of the file, which is {total_chars} characters long]"),
                truncated: false,
                total_chars,
                next_offset: None,
            })?);
        }

        let window: String = content.chars().skip(offset).take(MAX_READ_CHARS).collect();
        let end = offset + window.chars().count();
        let truncated = end < total_chars;

        let content = if truncated {
            format!(
                "{window}\n\n[... file truncated: showing characters {offset}-{end} of {total_chars}; \
                 call again with offset={end} to read on ...]"
            )
        } else {
            window
        };

        Ok(serde_json::to_value(ReadFileOut { content, truncated, total_chars, next_offset: truncated.then_some(end) })?)
    }
}
