use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::services::chat_store::HistoryDetail;
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct GetMessagesTool;

#[derive(Deserialize, ToolParams)]
struct GetMessagesArgs {
    #[tool(description = "The ids of the messages to fetch — the ids chat.list_messages returned. Pass every id you want in one call.")]
    #[serde(deserialize_with = "crate::tools::lenient::int_vec")]
    ids: Vec<i64>,
}

#[derive(Serialize)]
struct GetMessagesOut {
    /// The requested messages, oldest first.
    messages: Vec<HistoryDetail>,
    /// The requested ids that didn't match — not this chat's, or a tool result/notice,
    /// which aren't retrievable. Empty when every id matched.
    missing: Vec<i64>,
}

#[async_trait]
impl Tool for GetMessagesTool {
    fn function_name(&self) -> &str {
        "chat.get_messages"
    }

    fn description(&self) -> &str {
        "Fetches the full text of specific messages of this chat — the ids \
         chat.list_messages returned. Takes any mix of user and assistant message ids \
         in one call; this is how older messages come back into view, since they may \
         no longer be in the context window. An id that doesn't belong to this chat \
         (or is a tool result/notice, which aren't retrievable) comes back in `missing` \
         instead of failing the call."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        GetMessagesArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: GetMessagesArgs = serde_json::from_value(data)?;

        if args.ids.is_empty() {
            return Err(ToolError::FailedUnknown(
                "ids must not be empty — get them from chat.list_messages first".to_string(),
            ));
        }

        let (messages, missing) = ctx
            .chat_store
            .messages_by_ids(ctx.chat_id, &args.ids)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't fetch the messages: {e:?}")))?;

        Ok(serde_json::to_value(GetMessagesOut { messages, missing })?)
    }
}
