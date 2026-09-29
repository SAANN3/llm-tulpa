use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::services::chat_store::HistoryEntry;
use crate::tools::base::{
    PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams,
};

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;
const DEFAULT_OFFSET: u64 = 0;

pub struct ListMessagesTool;

#[derive(Deserialize, ToolParams)]
struct ListMessagesArgs {
    #[tool(description = "Only messages this one wrote: \"user\" or \"assistant\". Omit for both.")]
    role: Option<String>,
    #[tool(description = "How many messages per page. Default 50, at most 200.")]
    limit: Option<u64>,
    #[tool(description = "How many messages to skip from the newest end — the page to continue from. Default 0.")]
    offset: Option<u64>,
}

#[derive(Serialize)]
struct ListMessagesOut {
    messages: Vec<HistoryEntry>,
    /// Every user/assistant message of the chat the filter matches — not just this page.
    total: u64,
    /// Where this page started (what was passed as `offset`), so the pages line up.
    offset: u64,
    /// Where to continue from — `None` when this is the last page.
    next_offset: Option<u64>,
}

#[async_trait]
impl Tool for ListMessagesTool {
    fn function_name(&self) -> &str {
        "chat.list_messages"
    }

    fn description(&self) -> &str {
        "Lists this chat's messages — each as an id, who wrote it (user or assistant), \
         when, and a short snippet of what it says, newest first. Use this to find the \
         ids of messages you want the full text of (chat.get_messages) — e.g. to recall \
         what the user said or asked earlier in the chat, since older messages may no \
         longer be in the context window. `role` filters to one writer; `limit` and \
         `offset` paginate through it."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ListMessagesArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ListMessagesArgs = serde_json::from_value(data)?;

        let role = match args.role.as_deref().map(|r| r.trim().to_lowercase()) {
            None => None,
            Some(r) if r == "user" || r == "assistant" => Some(r),
            Some(r) => {
                return Err(ToolError::FailedUnknown(
                    format!("role must be \"user\" or \"assistant\", got \"{r}\""),
                ))
            }
        };

        let limit = args.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
        let offset = args.offset.unwrap_or(DEFAULT_OFFSET);

        let (messages, total) = ctx
            .chat_store
            .history_page(ctx.chat_id, role.as_deref(), limit, offset)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't list the chat's messages: {e:?}")))?;

        let fetched = offset + messages.len() as u64;
        let next_offset = (fetched < total).then_some(fetched);

        Ok(serde_json::to_value(ListMessagesOut { messages, total, offset, next_offset })?)
    }
}
