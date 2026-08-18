use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{services::error::ErrorService, state::AppState};

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
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_chat(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DeleteChatQuery>,
) -> Result<StatusCode, ErrorService> {
    state.chat_store.delete_chat(query.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
