use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::agent::ChatOut, routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkChoice, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct JobNoticesRequest {
    chat_id: i64,
    /// Ask the model to reason before answering. Defaults to enabled (Ollama's own
    /// default effort) when omitted. Either a plain bool, or a specific effort level
    /// string (e.g. `"low"`) — see `GET /api/llm/thinking_capability`.
    think: Option<ThinkChoice>,
}

/// Reports background jobs that have finished to `chat_id`'s model and returns its
/// reply — or `null`, without calling the model at all, if no job has finished since
/// the model was last told about one. What a client calls after `GET /api/events` says
/// `job_finished` while the chat has no turn running: safe to call on a stale or
/// duplicate hint, since whether there's anything to report is decided here. The reply
/// (and its `notices`) is shaped exactly like `POST /api/agent/continue`'s, and its
/// tool calls are driven the same way.
#[utoipa::path(
    post,
    path = "/api/agent/job_notices",
    tag = "agent",
    request_body = JobNoticesRequest,
    responses(
        (status = 200, description = "Model responded to the notices, or null if there was nothing to report", body = Option<ChatOut>),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach Ollama or the database", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn job_notices(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<JobNoticesRequest>,
) -> Result<Json<Option<ChatOut>>, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    let result = services.agent.run_pending_notices(body.chat_id, body.think).await?;

    Ok(Json(result))
}
