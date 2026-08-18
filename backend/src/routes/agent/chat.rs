use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::agent::ChatOut, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ChatRequest {
    chat_id: i64,
    prompt: String,
    /// Ask the model to reason before answering. Defaults to `true` when omitted.
    think: Option<bool>,
}

/// Sends `prompt` as the next turn in `chat_id`'s conversation and returns the model's
/// reply. If the reply carries tool calls, `can_use_tools` comes back `true` and the
/// caller drives `use_tool`/`can_use_tool` before asking for anything else.
#[utoipa::path(
    post,
    path = "/api/agent/chat",
    tag = "agent",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "Model responded", body = ChatOut),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach Ollama or the database", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let result = state.agent.chat(body.chat_id, body.prompt, body.think).await?;

    Ok(Json(result))
}
