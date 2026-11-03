use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::tool_selection_store::ToolMode, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct UserToolChoice {
    mode: ToolMode,
    tools: Vec<String>,
}

/// The tool selection a new chat copies.
#[utoipa::path(
    get,
    path = "/api/settings/tools",
    operation_id = "get_user_tools",
    tag = "settings",
    responses(
        (status = 200, description = "The user's default mode and the names it lists", body = UserToolChoice),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_user_tools(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<UserToolChoice>, ErrorService> {
    let services = state.services().await?;
    let mode = services.settings_store.tool_mode(auth.id).await?;
    let tools = services.tool_selection.names_for_user(auth.id).await?;
    Ok(Json(UserToolChoice { mode, tools }))
}
