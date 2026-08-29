use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use sea_orm::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct GetMessagesQuery {
    chat_id: i64,
    limit: Option<u64>,
    skip: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MessageToolCallOut {
    tool_name: String,
    #[schema(value_type = Object)]
    arguments: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MessageOut {
    id: i64,
    chat_id: i64,
    role: String,
    content: String,
    tool_name: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    created_at: DateTimeUtc,
    thinking: Option<String>,
    thought_duration_ms: Option<i64>,
    /// Only meaningful on a `tool`-role message — mirrors `UseToolOut.success` at the
    /// time this message was written. `null` for every other role.
    tool_success: Option<bool>,
    /// Only meaningful on a `tool`-role message with `tool_success: false` — mirrors
    /// `UseToolOut.denied`.
    tool_denied: bool,
    tool_calls: Vec<MessageToolCallOut>,
    /// Base64-encoded image data (no data-URL prefix) attached to this message, if
    /// any. Empty for every role but `user`.
    images: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MessagesResponse {
    messages: Vec<MessageOut>,
    total: u64,
}

/// A chat's messages, newest first — `skip` counts from the newest end, so `skip=0,
/// limit=50` gets the latest 50 and `skip=50, limit=50` gets the next 50 older ones.
/// Each message includes whatever tool calls it made, plus the total message count in
/// the chat for pagination.
#[utoipa::path(
    get,
    path = "/api/chats/messages",
    tag = "chats",
    params(GetMessagesQuery),
    responses(
        (status = 200, description = "Messages page", body = MessagesResponse),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetMessagesQuery>,
) -> Result<Json<MessagesResponse>, ErrorService> {
    let limit = query.limit.unwrap_or(50);
    let skip = query.skip.unwrap_or(0);

    let (messages, total) = state
        .chat_store
        .messages(query.chat_id, limit, skip)
        .await?;

    let messages = messages
        .into_iter()
        .map(|message| MessageOut {
            id: message.id,
            chat_id: message.chat_id,
            role: message.role,
            content: message.content,
            tool_name: message.tool_name,
            created_at: message.created_at,
            thinking: message.thinking,
            thought_duration_ms: message.thought_duration_ms,
            tool_success: message.tool_success,
            tool_denied: message.tool_denied,
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(|call| MessageToolCallOut {
                    tool_name: call.tool_name,
                    arguments: call.arguments,
                })
                .collect(),
            images: message.images,
        })
        .collect();

    Ok(Json(MessagesResponse { messages, total }))
}
