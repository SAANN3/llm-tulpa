use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams, ToolPermission, ToolSerializationError};

use super::{check_directory_scope, normalize};

pub struct FindFilesTool;

#[derive(Deserialize, ToolParams)]
struct FindFilesArgs {
    #[tool(description = "Directory to search in.")]
    directory: String,
    #[tool(
        description = "Only include files whose contents contain this substring, like \
                        grep. Files that aren't valid UTF-8 text are treated as \
                        non-matching rather than erroring the whole search. Optional."
    )]
    substr: Option<String>,
    #[tool(description = "Only include files whose name contains this substring. Optional.")]
    file_name: Option<String>,
    #[tool(
        description = "How many subdirectory levels below `directory` to search. 0 \
                        searches only `directory` itself. Unlimited if omitted."
    )]
    depth_limit: Option<i64>,
}

#[async_trait]
impl Tool for FindFilesTool {
    fn function_name(&self) -> &str {
        "storage.find_files"
    }

    fn description(&self) -> &str {
        "Searches a directory tree for files, like `find`/`grep` combined: filter by \
         filename substring, by file content substring, or both. Returns the matching \
         files' paths."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        FindFilesArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: Option<Value>,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: FindFilesArgs = serde_json::from_value(data)?;
        Ok(check_directory_scope(&args.directory, scope))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: FindFilesArgs = serde_json::from_value(data)?;
        let directory = normalize(std::path::Path::new(&args.directory));

        let matches = find_matches(
            directory.clone(),
            args.depth_limit,
            args.file_name.as_deref(),
            args.substr.as_deref(),
        )
        .await
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't search '{}': {e}", directory.display())))?;

        Ok(serde_json::to_value(matches)?)
    }
}

/// Walks `root` depth-first with an explicit stack (`Vec::pop` is LIFO) rather than
/// recursive `async fn` calls (which can't recurse directly — the resulting future
/// would be infinitely sized). `depth` counts subdirectory levels below `root`; `root`
/// itself is depth 0. Traversal order isn't part of this tool's contract — nothing
/// depends on it being depth-first specifically, that's just what an explicit stack
/// gives for free.
async fn find_matches(
    root: std::path::PathBuf,
    depth_limit: Option<i64>,
    file_name: Option<&str>,
    substr: Option<&str>,
) -> std::io::Result<Vec<String>> {
    let mut matches = Vec::new();
    let mut stack = vec![(root, 0i64)];

    while let Some((dir, depth)) = stack.pop() {
        let mut read_dir = tokio::fs::read_dir(&dir).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_dir() {
                if depth_limit.map(|limit| depth < limit).unwrap_or(true) {
                    stack.push((path, depth + 1));
                }
                continue;
            }

            let name_matches = file_name
                .map(|needle| entry.file_name().to_string_lossy().contains(needle))
                .unwrap_or(true);
            if !name_matches {
                continue;
            }

            let content_matches = match substr {
                None => true,
                Some(needle) => tokio::fs::read_to_string(&path)
                    .await
                    .map(|content| content.contains(needle))
                    .unwrap_or(false),
            };
            if !content_matches {
                continue;
            }

            matches.push(path.to_string_lossy().to_string());
        }
    }

    Ok(matches)
}
