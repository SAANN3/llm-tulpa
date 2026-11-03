use std::sync::Arc;

use axum::{extract::{Query, State}, Json};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::tool_selection_store::ToolMode, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct GetToolsQuery {
    chat_id: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ToolChoiceOut {
    mode: ToolMode,
    /// The names that mode lists.
    tools: Vec<String>,
}

/// The tool selection of one chat. The chat list itself carries only the mode.
#[utoipa::path(
    get,
    path = "/api/chats/tools",
    tag = "chats",
    params(GetToolsQuery),
    responses(
        (status = 200, description = "The chat's mode and the names it lists", body = ToolChoiceOut),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_tools(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<GetToolsQuery>,
) -> Result<Json<ToolChoiceOut>, ErrorService> {
    let services = state.services().await?;
    let chat = services.chat_store.owned_chat(auth.id, query.chat_id).await?;
    let tools = services.tool_selection.names_for_chat(chat.id).await?;
    Ok(Json(ToolChoiceOut { mode: chat.tool_mode, tools }))
}
