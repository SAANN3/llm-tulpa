use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::event_bus::ServerEvent, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RewindChatRequest {
    chat_id: i64,
    /// The first message to remove; it and every later message of the chat go.
    message_id: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RewindChatResponse {
    /// How many messages were removed
    deleted: u64,
}

/// Removes a message and everything after it, so the chat reads as it did just before that
/// message. Refused (`409`) when a tool call, tool result or job notice is part of what would
/// go, in a sub-agent's or a messaging plugin's chat, and for a message already folded into the
/// chat's summary.
#[utoipa::path(
    post,
    path = "/api/chats/rewind",
    tag = "chats",
    request_body = RewindChatRequest,
    responses(
        (status = 200, description = "Messages removed", body = RewindChatResponse),
        (status = 404, description = "No such chat or message", body = crate::services::error::ErrorBody),
        (status = 409, description = "That part of the chat can't be removed, or the chat has a run going on", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn rewind_chat(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<RewindChatRequest>,
) -> Result<Json<RewindChatResponse>, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    // The run is building on these messages: it would store its next reply after the gap
    if services.agent.has_run(body.chat_id) {
        return Err(ErrorService::new(StatusCode::CONFLICT, "the chat has a run going on"));
    }
    let removed = services.chat_store.rewind_from(body.chat_id, body.message_id).await?;
    if !removed.is_empty() {
        state.events.publish(ServerEvent::MessagesRemoved { chat_id: body.chat_id, message_ids: removed.clone() });
    }

    Ok(Json(RewindChatResponse { deleted: removed.len() as u64 }))
}
