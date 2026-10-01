use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::agent::ChatOut, routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkChoice, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RegenerateRequest {
    chat_id: i64,
    /// The reply to replace: the id of the chat's newest message, which must be a plain
    /// assistant reply (see the endpoint's description).
    message_id: i64,
    /// Same as on `/api/agent/chat`.
    think: Option<ThinkChoice>,
}

/// Has the model answer again and replaces the chat's last reply with the new answer. Only a
/// plain final reply can be regenerated: the newest message of the chat, written by the
/// assistant, with no tool calls, directly after a message of the user's and newer than the
/// chat's compaction boundary. The old reply is removed only once the new one is stored, so a
/// failure leaves the chat unchanged. The returned reply is handled like one from
/// `/api/agent/chat`, tool calls included.
#[utoipa::path(
    post,
    path = "/api/agent/regenerate",
    tag = "agent",
    request_body = RegenerateRequest,
    responses(
        (status = 200, description = "The new reply, which has replaced the old one", body = ChatOut),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 409, description = "That message isn't a reply that can be regenerated", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn regenerate(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<RegenerateRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    let result = services.agent.regenerate(body.chat_id, body.message_id, body.think).await?;

    Ok(Json(result))
}
