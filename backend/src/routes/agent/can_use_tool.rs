use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::agent::CanUseTool, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct CanUseToolRequest {
    chat_id: i64,
}

/// The tool calls the model has asked for in `chat_id` that haven't been run yet,
/// without running them — for checking `is_dangerous` before committing to `use_tool`.
#[utoipa::path(
    post,
    path = "/api/agent/can_use_tool",
    tag = "agent",
    request_body = CanUseToolRequest,
    responses(
        (status = 200, description = "Pending tool calls, if any", body = CanUseTool),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn can_use_tool(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CanUseToolRequest>,
) -> Result<Json<CanUseTool>, ErrorService> {
    let result = state.agent.can_use_tool(body.chat_id).await?;

    Ok(Json(result))
}
