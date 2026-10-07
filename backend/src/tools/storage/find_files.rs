use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};

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
         files' paths. Folders it can't open are skipped, and files over 16 MB aren't \
         searched for content."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        FindFilesArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: FindFilesArgs = serde_json::from_value(data)?;
        Ok(check_directory_scope(
            &args.directory,
            SharedBucket::StorageRead,
            scope.shared.get(&SharedBucket::StorageRead),
        ))
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
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

/// Files larger than this aren't read for a content search: the search reads each file whole, and a model
/// file or disk image in the tree would otherwise be pulled into memory.
const MAX_CONTENT_SEARCH_BYTES: u64 = 16 * 1024 * 1024;

/// Walks `root` depth-first with an explicit stack (`Vec::pop` is LIFO) rather than
/// recursive `async fn` calls (which can't recurse directly — the resulting future
/// would be infinitely sized). `depth` counts subdirectory levels below `root`; `root`
/// itself is depth 0. Traversal order isn't part of this tool's contract — nothing
/// depends on it being depth-first specifically, that's just what an explicit stack
/// gives for free. Only `root` itself has to open: a subfolder that can't (no permission, gone meanwhile)
/// is skipped, since one unreadable folder deep in a home directory used to fail the whole search.
async fn find_matches(
    root: std::path::PathBuf,
    depth_limit: Option<i64>,
    file_name: Option<&str>,
    substr: Option<&str>,
) -> std::io::Result<Vec<String>> {
    let mut matches = Vec::new();
    let mut stack = vec![(root.clone(), 0i64)];

    while let Some((dir, depth)) = stack.pop() {
        let mut read_dir = match tokio::fs::read_dir(&dir).await {
            Ok(read_dir) => read_dir,
            Err(e) if dir == root => return Err(e),
            // Skipped on purpose: see above
            Err(_) => continue,
        };

        loop {
            let entry = match read_dir.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => break,
            };
            let path = entry.path();
            // Doesn't follow symlinks (a symlinked folder isn't walked into, so no loops); an entry that went
            // away meanwhile is skipped
            let Ok(metadata) = entry.metadata().await else { continue };

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
                Some(_) if metadata.len() > MAX_CONTENT_SEARCH_BYTES => false,
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

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[tokio::test]
    async fn an_unreadable_folder_or_a_huge_file_does_not_end_the_search() {
        let root = std::env::temp_dir().join(format!("tulpa-find-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("open/deeper")).unwrap();
        std::fs::create_dir_all(root.join("closed")).unwrap();
        std::fs::write(root.join("open/deeper/a.txt"), "needle here").unwrap();
        std::fs::write(root.join("closed/b.txt"), "needle here").unwrap();
        // Starts with the needle, then grows past the content-search limit
        let big = root.join("open/big.bin");
        std::fs::write(&big, "needle").unwrap();
        std::fs::File::options().write(true).open(&big).unwrap().set_len(MAX_CONTENT_SEARCH_BYTES + 1).unwrap();
        std::fs::set_permissions(root.join("closed"), std::fs::Permissions::from_mode(0o000)).unwrap();

        let found = find_matches(root.clone(), None, None, Some("needle")).await;
        std::fs::set_permissions(root.join("closed"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = found.unwrap();
        assert_eq!(found, vec![root.join("open/deeper/a.txt").to_string_lossy().to_string()]);

        // The folder asked for is still an error when it can't be opened
        assert!(find_matches(root.join("missing"), None, None, None).await.is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
