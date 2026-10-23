//! What the model can do with skills. Every tool works on the skills of the user whose chat the call is in (`ctx.user_id`),
//! and the ones that change anything change only what the model made itself (`Origin::Model`): a skill the user uploaded
//! is read-only to it, which `SkillStore` enforces.

mod add_file;
mod delete;
mod disable;
mod install;
mod list;
mod read;
mod read_file;
mod write;

use std::sync::Arc;

use super::store::{SkillStore, SkillStoreError};
use crate::tools::base::{ResolvedScope, SharedBucket, Tool, ToolError, ToolPermission};
use crate::tools::storage::check_directory_scope;

pub fn collect(store: Arc<SkillStore>) -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(list::ListTool { store: store.clone() }),
        Box::new(read::ReadTool { store: store.clone() }),
        Box::new(read_file::ReadFileTool { store: store.clone() }),
        Box::new(write::WriteTool { store: store.clone() }),
        Box::new(add_file::AddFileTool { store: store.clone() }),
        Box::new(delete::DeleteTool { store: store.clone() }),
        Box::new(disable::DisableTool { store: store.clone() }),
        Box::new(install::InstallTool { store }),
    ]
}

fn failed(e: SkillStoreError) -> ToolError {
    ToolError::FailedUnknown(e.to_string())
}

/// Whether the user has let the model change files in their skills folder, asked the way every folder approval is: the
/// folder is that of the user the call is for, so a grant for one user's skills never covers another's.
fn check_folder(store: &SkillStore, user_id: i64, bucket: SharedBucket, scope: &ResolvedScope) -> ToolPermission {
    let folder = store.user_dir(user_id);
    check_directory_scope(&folder.to_string_lossy(), bucket, scope.shared.get(&bucket))
}

/// A file's text, or why the model cannot have it as text.
fn as_text(path: &str, bytes: Vec<u8>) -> Result<String, ToolError> {
    String::from_utf8(bytes).map_err(|e| ToolError::FailedUnknown(format!("'{path}' is not text ({} bytes)", e.as_bytes().len())))
}
