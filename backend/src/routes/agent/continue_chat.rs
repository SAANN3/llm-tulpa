use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::agent::ChatOut, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ContinueChatRequest {
    chat_id: i64,
    /// Ask the model to reason before answering. Defaults to `true` when omitted.
    think: Option<bool>,
}

/// Sends `chat_id`'s existing history to the model as-is, with no new turn added, and
/// returns its reply — for getting the model's next response after `use_tool` has
/// persisted a tool's result.
#[utoipa::path(
    post,
    path = "/api/agent/continue",
    tag = "agent",
    request_body = ContinueChatRequest,
    responses(
        (status = 200, description = "Model responded", body = ChatOut),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach Ollama or the database", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status — including calling this with nothing new since the chat's last assistant message, which Ollama rejects as two consecutive assistant messages", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn continue_chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ContinueChatRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let result = state.agent.continue_chat(body.chat_id, body.think).await?;

    Ok(Json(result))
}
