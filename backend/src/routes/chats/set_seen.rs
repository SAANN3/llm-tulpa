use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::event_bus::ServerEvent, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetSeenRequest {
    chat_id: i64,
}

/// Marks the chat as seen: clears `unseen_end`, the note of how its last run ended, and tells the user's other open
/// pages with a `chat_seen` event. Does nothing (and sends nothing) for a chat that has no note, and doesn't change
/// the chat's place in the list.
#[utoipa::path(
    post,
    path = "/api/chats/seen",
    tag = "chats",
    request_body = SetSeenRequest,
    responses(
        (status = 204, description = "The chat is marked as seen"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_seen(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetSeenRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    if services.chat_store.clear_unseen_end(body.chat_id).await? {
        state.events.publish(ServerEvent::ChatSeen { chat_id: body.chat_id });
    }

    Ok(StatusCode::NO_CONTENT)
}
