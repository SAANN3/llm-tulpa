use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

use super::get::ChatOut;

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateChatRequest {
    name: String,
}

/// Creates a new chat with the given name and returns its info.
#[utoipa::path(
    post,
    path = "/api/chats",
    tag = "chats",
    request_body = CreateChatRequest,
    responses(
        (status = 200, description = "Chat created", body = ChatOut),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateChatRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let chat = state.chat_store.create_chat(body.name).await?;

    Ok(Json(ChatOut {
        id: chat.id,
        name: chat.name,
        created_at: chat.created_at,
        updated_at: chat.updated_at,
    }))
}
