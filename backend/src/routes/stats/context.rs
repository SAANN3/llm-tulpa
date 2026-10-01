use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct ContextChatOut {
    chat_id: i64,
    name: String,
    /// What the model last evaluated for this chat, in tokens
    prompt_tokens: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ContextResponse {
    /// The context window the agent runs under, in tokens — what `prompt_tokens` fills
    context_length: u64,
    /// The user's biggest chats, largest first
    largest: Vec<ContextChatOut>,
    /// Chats whose older history has been folded into a summary
    compacted_chats: u64,
    total_chats: u64,
}

/// How full the calling user's chats are against the context window.
#[utoipa::path(
    get,
    path = "/api/stats/context",
    tag = "stats",
    responses(
        (status = 200, description = "Context usage", body = ContextResponse),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn context(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ContextResponse>, ErrorService> {
    let services = state.services().await?;
    let context = services.stats.context(auth.id).await?;

    Ok(Json(ContextResponse {
        context_length: context.context_length,
        largest: context
            .usage
            .largest
            .into_iter()
            .map(|chat| ContextChatOut { chat_id: chat.chat_id, name: chat.name, prompt_tokens: chat.prompt_tokens })
            .collect(),
        compacted_chats: context.usage.compacted_chats,
        total_chats: context.usage.total_chats,
    }))
}
