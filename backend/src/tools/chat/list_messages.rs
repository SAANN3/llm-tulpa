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
    #[tool(description = "Also list the assistant's intermediate steps, the messages that only carry tool calls. Default false: they are left out, so the list holds the user's messages and the assistant's actual replies.")]
    include_tool_calls: Option<bool>,
    #[tool(description = "How many messages per page. Default 50, at most 200.")]
    limit: Option<u64>,
    #[tool(description = "How many messages to skip from the newest end — the page to continue from. Default 0.")]
    offset: Option<u64>,
}

#[derive(Serialize)]
struct ListMessagesOut {
    messages: Vec<HistoryEntry>,
    /// Every listed message of the chat the filters match — not just this page.
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
        "Looks up this chat's own earlier messages. Call it whenever you need something said \
         earlier in this conversation that isn't in front of you — what the user originally \
         asked for, what they agreed to or ruled out, a detail they mentioned — and before \
         telling the user you can't see, remember, or verify the history; also to check your \
         work against the original request (nothing missed, nothing contradicted). Returns \
         each message as an id, who wrote it (user or assistant), when, and a short snippet, \
         newest first; pass the ids you want read in full to chat.get_messages. By default \
         only the user's messages and your actual replies are listed — the assistant steps \
         that only carry tool calls are left out, `include_tool_calls` lists them too. \
         `role` filters to one writer; `limit` and `offset` paginate."
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
            .history_page(ctx.chat_id, role.as_deref(), args.include_tool_calls.unwrap_or(false), limit, offset)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't list the chat's messages: {e:?}")))?;

        let fetched = offset + messages.len() as u64;
        let next_offset = (fetched < total).then_some(fetched);

        Ok(serde_json::to_value(ListMessagesOut { messages, total, offset, next_offset })?)
    }
}
