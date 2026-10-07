use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams,
    ToolPermission, ToolSerializationError,
};
use crate::tools::storage::{check_file_scope, normalize, refuse_if_larger};

/// The same as `routes::files`' upload limit: an attached file is a file in the chat like an uploaded one.
const MAX_ATTACH_BYTES: u64 = 500 * 1024 * 1024;

pub struct AttachFileTool;

#[derive(Deserialize, ToolParams)]
struct AttachFileArgs {
    #[tool(description = "Absolute or relative path to the existing file to attach.")]
    path: String,
}

#[derive(Serialize)]
struct AttachFileOut {
    file_id: i64,
    /// Repeated inside the result itself (not just this tool's description) — a model
    /// reads what a call actually returned more reliably than a description it saw
    /// before deciding to call it.
    note: &'static str,
}

#[async_trait]
impl Tool for AttachFileTool {
    fn function_name(&self) -> &str {
        "ui.attach_file"
    }

    fn description(&self) -> &str {
        "Attaches a file to your reply so the user sees it in the UI. Attach a file when the \
         user has no other way to get it: something you made for them to take away without \
         naming a place for it (a script, document, image or archive), or the result of a file \
         they sent through the chat. Don't attach a file you put where the user told you to, or \
         merely edited in their own project or working folder: it is already where they want \
         it, and most edits don't need this. Makes a snapshot copy of the file as it is right now, so tell the user that if it \
         matters — the copy shown won't reflect any changes made to the file afterward. \
         The file shows up together with your next real written reply, not this tool call \
         itself, so don't wait for it, mention it, or read/summarize/relay this result back \
         to the user yourself — just keep answering normally and it'll appear alongside what \
         you say. This is how you share files with the user: if they need a file \
         (a script, document, image, anything), attach it here rather than telling them \
         you can't 'create a link' or 'upload to a file host'. You can only attach files \
         you can already read."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        AttachFileArgs::tool_properties()
    }

    // The file being attached must already be one the model can read — same permission a
    // plain storage.read_file on this path would need, not a separate grant of its own.
    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: AttachFileArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, SharedBucket::StorageRead, scope.shared.get(&SharedBucket::StorageRead)))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: AttachFileArgs = serde_json::from_value(data)?;
        let path = normalize(Path::new(&args.path));
        // The same limit as a file the user uploads; the copy is read whole to store it
        refuse_if_larger(&path, MAX_ATTACH_BYTES, "to attach", "tell the user where it is instead").await?;

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;

        // The UI-facing name is just the path's own basename — nothing here needs the
        // model to invent or supply one, and it couldn't pick a more meaningful one
        // from the path alone anyway.
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let record = ctx
            .file_store
            .store_bytes(ctx.user_id, Some(ctx.chat_id), &file_name, &bytes, Some(true))
            .await
            .map_err(|e| {
                ToolError::FailedUnknown(format!("couldn't store a copy of '{}' to attach: {e:?}", path.display()))
            })?;

        Ok(serde_json::to_value(AttachFileOut {
            file_id: record.id,
            note: "Will be attached to your next real written reply automatically — don't read, \
                   describe, or repeat this file's contents or its id back to the user; just \
                   continue your reply normally.",
        })?)
    }
}
