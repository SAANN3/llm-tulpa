use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::{facade::agent::UseToolOut, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct UseToolRequest {
    chat_id: i64,
    /// One-time scope override for this call only — never persisted. Omit to run
    /// against whatever's already been granted via `allow_scope` (if anything).
    /// Passing this is also how a caller confirms it knows what it's authorizing.
    #[schema(value_type = Object)]
    scope: Option<Value>,
}

/// Runs the next pending tool call for `chat_id`, in the order the model requested them,
/// and persists its result — one call per request. `tools` in the response lists what's
/// still left; loop this endpoint until it's empty, then call `continue`. A call that
/// isn't permitted (see `AgentToolCall.permission` from `chat`/`can_use_tool`) doesn't
/// run — check `denied` in the response.
#[utoipa::path(
    post,
    path = "/api/agent/use_tool",
    tag = "agent",
    request_body = UseToolRequest,
    responses(
        (status = 200, description = "Tool ran, failed, or was denied (check `success`/`denied`)", body = UseToolOut),
        (status = 400, description = "No pending tool call to run", body = crate::services::error::ErrorBody),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn use_tool(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UseToolRequest>,
) -> Result<Json<UseToolOut>, ErrorService> {
    let result = state.agent.use_tool(body.chat_id, body.scope).await?;

    Ok(Json(result))
}
