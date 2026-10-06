use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct StopRequest {
    chat_id: i64,
}

/// Stops the run on `chat_id`: the model call in flight is dropped (the server stops generating) and nothing of it
/// is stored; a tool that is already running finishes and its result is stored. The chat is left as of its last
/// stored message, and `run_ended` with the reason `stopped` follows.
#[utoipa::path(
    post,
    path = "/api/agent/stop",
    tag = "agent",
    request_body = StopRequest,
    responses(
        (status = 204, description = "The run was told to stop"),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has no run to stop", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn stop(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<StopRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.agent.stop(body.chat_id)?;
    Ok(StatusCode::NO_CONTENT)
}
