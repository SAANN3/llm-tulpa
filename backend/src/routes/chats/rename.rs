use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RenameChatRequest {
    chat_id: i64,
    name: String,
}

/// Renames a chat.
#[utoipa::path(
    post,
    path = "/api/chats/rename",
    tag = "chats",
    request_body = RenameChatRequest,
    responses(
        (status = 204, description = "Chat renamed"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn rename_chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RenameChatRequest>,
) -> Result<StatusCode, ErrorService> {
    state.chat_store.rename_chat(body.chat_id, body.name).await?;

    Ok(StatusCode::NO_CONTENT)
}
