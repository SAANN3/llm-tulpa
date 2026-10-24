use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct UninstallBody {
    pub plugin_name: String,
    pub plugin_subname: String,
    /// Also delete what the plugin kept: its settings, its database schema and its data folder. Without it they stay, and
    /// installing the plugin again finds them.
    #[serde(default)]
    pub remove_data: bool,
}

/// Uninstalls an external plugin: it is unloaded and its folder and record are deleted
#[utoipa::path(
    post,
    path = "/api/plugins/external/uninstall",
    tag = "plugins",
    request_body = UninstallBody,
    responses(
        (status = 204, description = "Uninstalled"),
        (status = 400, description = "An install or uninstall is running", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such external plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn uninstall_external_plugin(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<UninstallBody>) -> Result<StatusCode, ErrorService> {
    state.services().await?.external_plugins.uninstall(&body.plugin_name, &body.plugin_subname, body.remove_data).await?;
    Ok(StatusCode::NO_CONTENT)
}
