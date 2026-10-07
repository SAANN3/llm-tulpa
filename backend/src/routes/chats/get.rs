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
pub(crate) struct GetChatsQuery {
    /// A specific chat's id. Given alone, the response is that chat's full info instead
    /// of a list.
    id: Option<i64>,
    /// Scopes the list to one folder's chats. Omitted → every non-deleted chat regardless
    /// of folder (each still reports its own `folder_id` for client-side grouping).
    folder_id: Option<i64>,
    limit: Option<u64>,
    skip: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatOut {
    pub(crate) id: i64,
    pub(crate) name: String,
    /// The model this chat is bound to, and the provider it belongs to.
    pub(crate) model: String,
    pub(crate) provider: String,
    #[schema(value_type = String, format = "date-time")]
    pub(crate) created_at: DateTimeUtc,
    #[schema(value_type = String, format = "date-time")]
    pub(crate) updated_at: DateTimeUtc,
    /// Ollama's measured prompt size (prompt + generated tokens) at the end of this
    /// chat's last model call — the current context usage. `null` until the chat has
    /// had at least one turn Ollama reported metrics for.
    pub(crate) last_prompt_tokens: Option<i64>,
    /// The context window the agent runs under — the ceiling `last_prompt_tokens` is
    /// budgeted against, and the "max" half of a context usage display.
    pub(crate) context_length: u64,
    /// The folder this chat is grouped under, or `null` if ungrouped.
    pub(crate) folder_id: Option<i64>,
    /// The chat that started this one as a sub-agent, or `null` for an ordinary chat. A
    /// sub-agent's chat is not in the chat list; it is reached from its parent.
    pub(crate) parent_chat_id: Option<i64>,
    /// The launch profile this chat runs on (which implies its model), or `null` for a chat on a
    /// model that has none.
    pub(crate) launch_profile_id: Option<i64>,
    /// Whether the model is sent its tools in this chat.
    pub(crate) tools_enabled: bool,
    /// How the last run on this chat ended, until the chat is opened (`POST /api/chats/seen`): `answered`,
    /// `waiting_for_permission`, `failed` or `step_limit`. `null` when there is nothing new.
    pub(crate) unseen_end: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatListOut {
    chats: Vec<ChatOut>,
    total: u64,
}

/// `id` present → a single chat's full info. `id` absent → the paginated chat list.
/// Different shapes for the same route rather than two routes, since the caller already
/// asked one specific question either way ("this chat" vs "which chats exist").
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum GetChatsResponse {
    Single(ChatOut),
    List(ChatListOut),
}

/// `id` given → that chat's info (404 if it doesn't exist or is deleted). `id` omitted →
/// a page of non-deleted chats, newest-active first, plus a total count for pagination.
#[utoipa::path(
    get,
    path = "/api/chats",
    tag = "chats",
    params(GetChatsQuery),
    responses(
        (status = 200, description = "A single chat, or a page of chats", body = GetChatsResponse),
        (status = 404, description = "`id` given but no such chat exists", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_chats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<GetChatsQuery>,
) -> Result<Json<GetChatsResponse>, ErrorService> {
    let services = state.services().await?;

    if let Some(id) = query.id {
        let chat = services.chat_store.owned_chat(auth.id, id).await?;
        let contexts = services.launches.contexts(services.context_length).await?;

        return Ok(Json(GetChatsResponse::Single(ChatOut {
            id: chat.id,
            name: chat.name,
            model: chat.model,
            provider: chat.provider,
            created_at: chat.created_at,
            updated_at: chat.updated_at,
            last_prompt_tokens: chat.last_prompt_tokens,
            context_length: contexts.for_profile(chat.launch_profile_id),
            folder_id: chat.folder_id,
            parent_chat_id: chat.parent_chat_id,
            launch_profile_id: chat.launch_profile_id,
            tools_enabled: chat.tools_enabled,
            unseen_end: chat.unseen_end,
        })));
    }

    let limit = query.limit.unwrap_or(50);
    let skip = query.skip.unwrap_or(0);
    let folder_id = query.folder_id.map(Some);

    let (chats, total) = services.chat_store.chats(auth.id, folder_id, limit, skip).await?;

    let contexts = services.launches.contexts(services.context_length).await?;
    let chats = chats
        .into_iter()
        .map(|chat| ChatOut {
            id: chat.id,
            name: chat.name,
            model: chat.model,
            provider: chat.provider,
            created_at: chat.created_at,
            updated_at: chat.updated_at,
            last_prompt_tokens: chat.last_prompt_tokens,
            context_length: contexts.for_profile(chat.launch_profile_id),
            folder_id: chat.folder_id,
            parent_chat_id: chat.parent_chat_id,
            launch_profile_id: chat.launch_profile_id,
            tools_enabled: chat.tools_enabled,
            unseen_end: chat.unseen_end,
        })
        .collect();

    Ok(Json(GetChatsResponse::List(ChatListOut { chats, total })))
}
