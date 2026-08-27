use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct SetPluginEnabledBody {
    pub plugin_name: String,
    pub plugin_subname: String,
    pub enabled: bool,
}

/// Enables or disables a registered plugin, running its `on_enabled`/`on_disabled`
/// hook (whichever applies) as part of the transition. `enabled: true` fails with 400
/// if the plugin has never been given settings — see `PluginRegistry::set_enabled`.
#[utoipa::path(
    post,
    path = "/api/plugins/enable",
    tag = "plugins",
    request_body = SetPluginEnabledBody,
    responses(
        (status = 204, description = "Enabled state changed"),
        (status = 400, description = "Can't enable a plugin with no settings configured yet", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such plugin", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_plugin_enabled(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetPluginEnabledBody>,
) -> Result<StatusCode, ErrorService> {
    state
        .plugin_registry
        .set_enabled(&body.plugin_name, &body.plugin_subname, body.enabled)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
