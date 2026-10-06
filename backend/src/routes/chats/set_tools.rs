use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetToolsRequest {
    chat_id: i64,
    /// Whether the model is sent its tools in this chat from now on.
    enabled: bool,
}

/// Turns the model's tools on or off for one chat. Takes effect with the chat's next request, which
/// changes the front of the prompt, so the model server reads the whole prompt once more. Refused (409)
/// while the chat has a run going on.
#[utoipa::path(
    post,
    path = "/api/chats/tools",
    tag = "chats",
    request_body = SetToolsRequest,
    responses(
        (status = 204, description = "Setting saved"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has a run going on", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_tools(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetToolsRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    if services.agent.has_run(body.chat_id) {
        return Err(ErrorService::new(StatusCode::CONFLICT, "the chat has a run going on"));
    }
    services.chat_store.set_tools_enabled(body.chat_id, body.enabled).await?;

    Ok(StatusCode::NO_CONTENT)
}
