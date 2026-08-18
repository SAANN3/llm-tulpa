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
pub(crate) struct GetChatsQuery {
    /// A specific chat's id. Given alone, the response is that chat's full info instead
    /// of a list.
    id: Option<i64>,
    limit: Option<u64>,
    skip: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatOut {
    pub(crate) id: i64,
    pub(crate) name: String,
    #[schema(value_type = String, format = "date-time")]
    pub(crate) created_at: DateTimeUtc,
    #[schema(value_type = String, format = "date-time")]
    pub(crate) updated_at: DateTimeUtc,
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
    Query(query): Query<GetChatsQuery>,
) -> Result<Json<GetChatsResponse>, ErrorService> {
    if let Some(id) = query.id {
        let chat = state.chat_store.chat(id).await?;

        return Ok(Json(GetChatsResponse::Single(ChatOut {
            id: chat.id,
            name: chat.name,
            created_at: chat.created_at,
            updated_at: chat.updated_at,
        })));
    }

    let limit = query.limit.unwrap_or(50);
    let skip = query.skip.unwrap_or(0);

    let (chats, total) = state.chat_store.chats(limit, skip).await?;

    let chats = chats
        .into_iter()
        .map(|chat| ChatOut {
            id: chat.id,
            name: chat.name,
            created_at: chat.created_at,
            updated_at: chat.updated_at,
        })
        .collect();

    Ok(Json(GetChatsResponse::List(ChatListOut { chats, total })))
}
