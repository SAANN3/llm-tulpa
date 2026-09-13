use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};

use super::{check_directory_scope, human_size, normalize};

pub struct ListDirectoryTool;

/// Hard ceiling on how many entries `list_directory` hands back in one call. A
/// directory with thousands of files (a whole home directory, a `node_modules`) could
/// otherwise single-handedly blow the model's context budget in one tool result — this
/// keeps any one call bounded regardless of how big the real directory is, with
/// `offset` (below) letting the model page through the rest on request instead.
const MAX_LIST_ENTRIES: usize = 200;

#[derive(Deserialize, ToolParams)]
struct ListDirectoryArgs {
    #[tool(
        description = "Absolute or relative path to the directory to list. Lists only \
                        its immediate contents, not subdirectories' contents."
    )]
    path: String,
    #[tool(
        description = "How many entries (sorted by name) to skip from the start, for \
                        paging through a directory with more than 200 entries — pass \
                        the `next_offset` a previous call returned to continue where it \
                        left off. Defaults to 0."
    )]
    offset: Option<i64>,
}

#[derive(Serialize)]
struct EntryOut {
    name: String,
    is_dir: bool,
    size: String,
    modified: Option<String>,
    readonly: bool,
}

#[derive(Serialize)]
struct ListDirectoryOut {
    entries: Vec<EntryOut>,
    /// Set only when this directory has more entries past what's returned here — the
    /// model needs to know its view is partial and how to see the rest, not just get
    /// silently handed less than what's actually there.
    note: Option<String>,
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn function_name(&self) -> &str {
        "storage.list_directory"
    }

    fn description(&self) -> &str {
        "Lists the immediate contents of a directory (like `ls -lsh`): each entry's \
         name, whether it's a directory, its size, when it was last modified, and \
         whether it's read-only. Returns at most 200 entries (sorted by name) per \
         call — a directory with more than that comes back with a `note` saying how \
         many are left and what `offset` to pass to keep paging through them."
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

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
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

        // Sorted rather than left in whatever order `read_dir` happened to yield —
        // that order isn't guaranteed stable, and pagination via `offset` only makes
        // sense (no gaps, no repeats across calls) against a consistent ordering.
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let offset = args.offset.unwrap_or(0).max(0) as usize;
        let total = entries.len();
        let page: Vec<EntryOut> = entries.into_iter().skip(offset).take(MAX_LIST_ENTRIES).collect();
        let remaining = total.saturating_sub(offset + page.len());
        let note = (remaining > 0).then(|| {
            format!(
                "{remaining} more entries not shown — call again with offset={} to see them.",
                offset + page.len()
            )
        });

        Ok(serde_json::to_value(ListDirectoryOut { entries: page, note })?)
    }
}
