use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::AuthUser,
    services::error::ErrorService,
    services::tool_selection_store::{ToolMode, ToolSelectionStore},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetToolsRequest {
    chat_id: i64,
    mode: ToolMode,
    /// The names this mode lists. Replaces the chat's list. Empty is every tool, or none, depending on `mode`.
    #[serde(default)]
    tools: Vec<String>,
}

/// Replaces which tools a chat sends the model. Refused while a run is going: the request it is
/// building still has the previous list.
#[utoipa::path(
    post,
    path = "/api/chats/tools",
    tag = "chats",
    request_body = SetToolsRequest,
    responses(
        (status = 204, description = "The chat's tool selection was replaced"),
        (status = 400, description = "A tool name that isn't one", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 409, description = "A run is going on this chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_tools(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetToolsRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    let chat = services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    if services.agent.has_run(chat.id) {
        return Err(ErrorService::new(StatusCode::CONFLICT, "the chat has a run going on"));
    }
    ToolSelectionStore::check_names(&body.tools)?;
    services.chat_store.set_tool_mode(chat.id, body.mode).await?;
    services.tool_selection.replace_chat(chat.id, &body.tools).await?;
    Ok(StatusCode::NO_CONTENT)
}
