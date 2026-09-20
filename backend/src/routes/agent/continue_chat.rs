use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::agent::ChatOut, routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkChoice, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ContinueChatRequest {
    chat_id: i64,
    /// Ask the model to reason before answering. Defaults to enabled (Ollama's own
    /// default effort) when omitted. Either a plain bool, or a specific effort level
    /// string (e.g. `"low"`) — see `GET /api/llm/thinking_capability`.
    think: Option<ThinkChoice>,
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
    auth: AuthUser,
    Json(body): Json<ContinueChatRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    let result = services.agent.continue_chat(body.chat_id, body.think).await?;

    Ok(Json(result))
}
