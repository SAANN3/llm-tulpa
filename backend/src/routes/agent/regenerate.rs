use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkChoice, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RegenerateRequest {
    chat_id: i64,
    /// The reply to replace: the id of the chat's newest message, which must be a plain
    /// assistant reply (see the endpoint's description).
    message_id: i64,
    /// Same as on `POST /api/agent/turn`.
    think: Option<ThinkChoice>,
}

/// Has the model answer again and replaces the chat's last reply with the new answer. Only a
/// plain final reply can be regenerated: the newest message of the chat, written by the
/// assistant, with no tool calls, directly after a message of the user's and newer than the
/// chat's compaction boundary. The old reply is removed only once the new one is stored, so a
/// failure leaves the chat unchanged. Returns at once, and the run goes on like one started by
/// `POST /api/agent/turn`, tool calls included.
#[utoipa::path(
    post,
    path = "/api/agent/regenerate",
    tag = "agent",
    request_body = RegenerateRequest,
    responses(
        (status = 202, description = "The run has started"),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 409, description = "That message isn't a reply that can be regenerated, or the chat has a run going on", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn regenerate(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<RegenerateRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.agent.start_regenerate(body.chat_id, body.message_id, body.think).await?;

    Ok(StatusCode::ACCEPTED)
}
