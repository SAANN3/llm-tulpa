use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::AuthUser,
    services::error::ErrorService,
    services::settings_store::SettingsUpdate,
    services::tool_selection_store::{ToolMode, ToolSelectionStore},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetUserToolChoice {
    mode: ToolMode,
    #[serde(default)]
    tools: Vec<String>,
}

/// Replaces the tool selection a new chat copies.
#[utoipa::path(
    post,
    path = "/api/settings/tools",
    operation_id = "set_user_tools",
    tag = "settings",
    request_body = SetUserToolChoice,
    responses(
        (status = 204, description = "The default was replaced"),
        (status = 400, description = "A tool name that isn't one", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_user_tools(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetUserToolChoice>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    ToolSelectionStore::check_names(&body.tools)?;
    let mut update = SettingsUpdate::default();
    update.tool_mode = Some(body.mode);
    services.settings_store.update(auth.id, update).await?;
    services.tool_selection.replace_user(auth.id, &body.tools).await?;
    Ok(StatusCode::NO_CONTENT)
}
