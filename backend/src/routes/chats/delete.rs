use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DeleteChatQuery {
    id: i64,
}

/// Soft-deletes a chat — it stops showing up in `GET /chats`, but its row and messages
/// stay in the database.
#[utoipa::path(
    delete,
    path = "/api/chats",
    tag = "chats",
    params(DeleteChatQuery),
    responses(
        (status = 204, description = "Chat deleted"),
        (status = 404, description = "No such chat, or already deleted", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has a run going on", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_chat(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DeleteChatQuery>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, query.id).await?;
    if services.agent.has_run(query.id) {
        return Err(ErrorService::new(StatusCode::CONFLICT, "the chat has a run going on"));
    }
    services.chat_store.delete_chat(query.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
