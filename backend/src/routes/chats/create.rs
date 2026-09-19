use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

use super::get::ChatOut;

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateChatRequest {
    name: String,
}

/// Creates a new chat with the given name and returns its info. The chat is bound to the
/// user's active model (else the first registered one) at creation.
#[utoipa::path(
    post,
    path = "/api/chats",
    tag = "chats",
    request_body = CreateChatRequest,
    responses(
        (status = 200, description = "Chat created", body = ChatOut),
        (status = 409, description = "No model has been selected yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_chat(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateChatRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let chat = state.services().await?.chat_store.create_chat(auth.id, body.name).await?;

    Ok(Json(ChatOut {
        id: chat.id,
        name: chat.name,
        model: chat.model,
        provider: chat.provider,
        created_at: chat.created_at,
        updated_at: chat.updated_at,
    }))
}
