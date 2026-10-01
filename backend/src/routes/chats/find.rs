use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use sea_orm::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

const DEFAULT_LIMIT: u64 = 30;
const MAX_LIMIT: u64 = 100;
/// How many matching messages each chat shows (the rest are only counted)
const MESSAGES_PER_CHAT: usize = 3;

#[derive(Deserialize, IntoParams)]
pub(crate) struct FindChatsQuery {
    /// The text to look for, case-insensitive
    query: String,
    /// Also look in the chats' messages, not only their names (default: true)
    include_messages: Option<bool>,
    /// How many chats to return (default 30, at most 100)
    limit: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatFindMessageOut {
    id: i64,
    role: String,
    #[schema(value_type = String, format = "date-time")]
    created_at: DateTimeUtc,
    /// The content before the matched text, trimmed to a snippet; null when the match starts the message
    before: Option<String>,
    matched: String,
    /// The content after the matched text, trimmed to a snippet; null when the match ends the message
    after: Option<String>,
    /// Always "content": only a message's own text is searched here
    matched_in: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatFindOut {
    chat_id: i64,
    name: String,
    #[schema(value_type = String, format = "date-time")]
    updated_at: DateTimeUtc,
    /// Whether the chat's name contains the text
    name_matched: bool,
    /// How many of the chat's messages contain it
    message_matches: u64,
    /// The newest matching messages, a few of them
    messages: Vec<ChatFindMessageOut>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FindChatsResponse {
    /// Most recently active first
    chats: Vec<ChatFindOut>,
}

/// The calling user's chats whose name contains `query`, and (unless turned off) those with a
/// message containing it. Sub-agent and deleted chats are left out.
#[utoipa::path(
    get,
    path = "/api/chats/find",
    tag = "chats",
    params(FindChatsQuery),
    responses(
        (status = 200, description = "Matching chats", body = FindChatsResponse),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn find_chats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<FindChatsQuery>,
) -> Result<Json<FindChatsResponse>, ErrorService> {
    let text = query.query.trim();
    if text.is_empty() {
        return Ok(Json(FindChatsResponse { chats: vec![] }));
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;

    let services = state.services().await?;
    let found = services
        .chat_store
        .find_chats(auth.id, text, query.include_messages.unwrap_or(true), limit, MESSAGES_PER_CHAT)
        .await?;

    let chats = found
        .into_iter()
        .map(|chat| ChatFindOut {
            chat_id: chat.chat_id,
            name: chat.name,
            updated_at: chat.updated_at,
            name_matched: chat.name_matched,
            message_matches: chat.message_matches,
            messages: chat
                .messages
                .into_iter()
                .map(|message| ChatFindMessageOut {
                    id: message.id,
                    role: message.role,
                    created_at: message.created_at,
                    before: message.before,
                    matched: message.matched,
                    after: message.after,
                    matched_in: "content".to_string(),
                })
                .collect(),
        })
        .collect();

    Ok(Json(FindChatsResponse { chats }))
}
