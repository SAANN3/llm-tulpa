use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct AllowScopeRequest {
    chat_id: i64,
    tool_name: String,
    #[schema(value_type = Object)]
    scope: Value,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AllowScopeOut {}

/// Persists a scope grant for `tool_name` within `chat_id`, so future calls to that
/// tool in that chat can run without asking again — see `AgentToolCall.permission`'s
/// `escalation.scope` (from `chat`/`can_use_tool`) for what to pass here. Doesn't run
/// anything itself; call `use_tool` separately afterward.
#[utoipa::path(
    post,
    path = "/api/agent/allow_scope",
    tag = "agent",
    request_body = AllowScopeRequest,
    responses(
        (status = 200, description = "Scope granted", body = AllowScopeOut),
        (status = 400, description = "No tool registered with that name", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn allow_scope(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AllowScopeRequest>,
) -> Result<Json<AllowScopeOut>, ErrorService> {
    state.agent.allow_scope(body.chat_id, body.tool_name, body.scope).await?;

    Ok(Json(AllowScopeOut {}))
}
