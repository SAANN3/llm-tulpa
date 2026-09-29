use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use sea_orm::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct SearchMessagesQuery {
    chat_id: i64,
    /// The keyword or phrase to find in the chat's messages — content, thinking and
    /// tool call arguments (case-insensitive)
    query: String,
    limit: Option<u64>,
    /// Whether to include assistant messages in the search (default: true)
    include_assistant: Option<bool>,
    /// Whether to include user messages in the search (default: true)
    include_user: Option<bool>,
    /// Whether to include assistant reasoning/thinking in the search (default: true)
    include_thinking: Option<bool>,
    /// Whether to include tool call arguments in the search (default: true)
    include_tools: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MessageSearchOut {
    id: i64,
    role: String,
    #[schema(value_type = String, format = "date-time")]
    created_at: DateTimeUtc,
    /// The content before the matched text, trimmed to a snippet; null when the match
    /// starts at the very beginning of the message.
    before: Option<String>,
    /// The matched text itself (the searched keyword, as it appears in the message).
    matched: String,
    /// The content after the matched text, trimmed to a snippet; null when the match
    /// ends at the very end of the message.
    after: Option<String>,
    /// Where the match was found: "content", "thinking", or "arguments" (a tool
    /// call's) — content takes priority, then thinking, then arguments.
    matched_in: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct MessageSearchResponse {
    matches: Vec<MessageSearchOut>,
    total: u64,
}

/// A chat's messages whose content, thinking or tool call arguments contain
/// `query`, newest first — each with a snippet around its first match, plus the
/// total match count in the chat.
#[utoipa::path(
    get,
    path = "/api/chats/search",
    tag = "chats",
    params(SearchMessagesQuery),
    responses(
        (status = 200, description = "Matches", body = MessageSearchResponse),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn search_messages(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<SearchMessagesQuery>,
) -> Result<Json<MessageSearchResponse>, ErrorService> {
    let limit = query.limit.unwrap_or(50);
    let include_assistant = query.include_assistant.unwrap_or(true);
    let include_user = query.include_user.unwrap_or(true);
    let include_thinking = query.include_thinking.unwrap_or(true);
    let include_tools = query.include_tools.unwrap_or(true);

    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, query.chat_id).await?;
    let (matches, total) = services
        .chat_store
        .search_messages(
            query.chat_id,
            &query.query,
            limit,
            include_assistant,
            include_user,
            include_thinking,
            include_tools,
        )
        .await?;

    let matches = matches
        .into_iter()
        .map(|hit| MessageSearchOut {
            id: hit.id,
            role: hit.role,
            created_at: hit.created_at,
            before: hit.before,
            matched: hit.matched,
            after: hit.after,
            matched_in: hit.matched_in.to_string(),
        })
        .collect();

    Ok(Json(MessageSearchResponse { matches, total }))
}
