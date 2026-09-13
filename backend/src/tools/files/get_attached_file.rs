use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct GetAttachedFileTool;

#[derive(Deserialize, ToolParams)]
struct GetAttachedFileArgs {
    #[tool(description = "The id of a file attached to this chat, as given in a message's own \"file(s) attached\" note.")]
    file_id: i64,
}

#[derive(Serialize)]
struct GetAttachedFileOut {
    file_id: i64,
    file_name: String,
    path: String,
    /// Repeated inside the result itself (not just this tool's description) — a model
    /// reads what a call actually returned more reliably than a description it saw
    /// before deciding to call it.
    note: &'static str,
}

#[async_trait]
impl Tool for GetAttachedFileTool {
    fn function_name(&self) -> &str {
        "files.get_attached_file"
    }

    fn description(&self) -> &str {
        "Gives you a real, readable copy of a file the user attached to this chat, by the id a \
         message's own \"file(s) attached\" note gave you — not the file's content itself, just \
         a path. Follow up with storage.detect_file_type (if you don't already know the format) \
         and storage.read_file to actually see what's in it; that first read only ever needs a \
         one-time folder approval, which then covers every attached file after that too. Only \
         works for a file actually attached to *this* chat — an id from anywhere else fails."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        GetAttachedFileArgs::tool_properties()
    }

    // No permission gating of its own: this only ever touches a file already attached
    // to the chat this call is already happening in (the user's own earlier action,
    // not a new grant the model is asking for) — the actual *read* afterward still
    // goes through storage.read_file's own normal StorageRead gate on wherever the
    // copy lands.
    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: GetAttachedFileArgs = serde_json::from_value(data)?;

        let not_attached = || ToolError::FailedUnknown("no such file attached to this chat".to_string());

        let record = ctx.file_store.get(args.file_id).await.map_err(|_| not_attached())?;

        // Deliberately the exact same error as "doesn't exist at all" above — a file
        // that's real but belongs to a different chat should look identical to one
        // that was never real, not confirm its existence to a chat it isn't in.
        if record.chat_id != Some(ctx.chat_id) {
            return Err(not_attached());
        }

        let copy = ctx
            .file_store
            .copy(args.file_id)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't prepare that file for reading: {e:?}")))?;

        Ok(serde_json::to_value(GetAttachedFileOut {
            file_id: copy.id,
            file_name: copy.file_name,
            path: copy.full_path,
            note: "This is only the file's location, not its content — read `path` with \
                   storage.read_file (storage.detect_file_type first if you're unsure of its \
                   format) to actually see what's in it.",
        })?)
    }
}
