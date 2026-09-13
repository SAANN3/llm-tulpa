pub mod get_attached_file;

use super::base::Tool;

/// Every tool in the `files` domain (function names prefixed `files.`) — tools for
/// working with `FileStore`-tracked files (already attached to a chat, referenced by
/// id), as opposed to `storage.*` (arbitrary real filesystem paths the model names
/// itself). For `main.rs` to register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![Box::new(get_attached_file::GetAttachedFileTool)]
}
