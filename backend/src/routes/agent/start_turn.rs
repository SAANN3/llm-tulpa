use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkChoice, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct StartTurnRequest {
    chat_id: i64,
    prompt: String,
    /// Base64-encoded image data (no data-URL prefix), one entry per attached image.
    /// Requires a vision-capable model — see `llm/README.md`.
    #[serde(default)]
    images: Option<Vec<String>>,
    /// Ids of files already uploaded via `POST /files/upload` to attach to this
    /// message. The model is told they are attached and reads one with `files.get_attached_file`.
    #[serde(default)]
    file_ids: Option<Vec<i64>>,
    /// Ask the model to reason before answering. Defaults to enabled (the provider's own
    /// default effort) when omitted. Either a plain bool, or a specific effort level
    /// string (e.g. `"low"`) — see `GET /api/llm/thinking_capability` for what the
    /// active model actually supports before sending one of these.
    think: Option<ThinkChoice>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct StartTurnOut {
    /// The id of the stored message with the user's prompt.
    user_message_id: i64,
}

/// Stores `prompt` as the user's next message and starts a run on `chat_id` that answers it: the
/// backend asks the model, runs the tools it asks for and asks again until it answers, without a
/// browser holding anything open. Returns at once; follow the run with `GET /api/live`
/// (`run_started`, `messages_changed`, `tool_started`, `run_ended`) and read what it stored through
/// the chat's messages. A tool call that needs permission ends the run until `POST /api/agent/answer`.
#[utoipa::path(
    post,
    path = "/api/agent/turn",
    tag = "agent",
    request_body = StartTurnRequest,
    responses(
        (status = 202, description = "The prompt is stored and the run has started", body = StartTurnOut),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat already has a run going on, or belongs to a messaging plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn start_turn(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<StartTurnRequest>,
) -> Result<(StatusCode, Json<StartTurnOut>), ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    let started = services
        .agent
        .start_turn(
            body.chat_id,
            body.prompt,
            body.images.unwrap_or_default(),
            body.file_ids.unwrap_or_default(),
            body.think,
        )
        .await?;

    Ok((StatusCode::ACCEPTED, Json(StartTurnOut { user_message_id: started.user_message_id })))
}
