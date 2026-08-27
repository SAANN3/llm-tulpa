use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct SetPluginSettingsBody {
    pub plugin_name: String,
    pub plugin_subname: String,
    /// Validated against this plugin's own schema (see `GET /api/plugins/settings_schema`)
    /// by `PluginBuilder::build`, not here — a mismatch surfaces as a 400 either way.
    #[schema(value_type = Object)]
    pub settings: Value,
}

/// Sets (or replaces) a plugin's settings, rebuilding its live instance from them —
/// works both for a plugin that's never been configured yet and one that already has
/// settings, same as `PluginRegistry::update_settings`. Doesn't change whether the
/// plugin is enabled; use `POST /api/plugins/enable` for that.
#[utoipa::path(
    post,
    path = "/api/plugins/settings",
    tag = "plugins",
    request_body = SetPluginSettingsBody,
    responses(
        (status = 204, description = "Settings saved and the plugin instance rebuilt"),
        (status = 400, description = "Settings didn't match this plugin's schema", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such plugin", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_plugin_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetPluginSettingsBody>,
) -> Result<StatusCode, ErrorService> {
    state
        .plugin_registry
        .update_settings(&body.plugin_name, &body.plugin_subname, body.settings)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
