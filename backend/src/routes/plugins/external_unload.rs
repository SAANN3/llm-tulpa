use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use super::external_load::ExternalPluginBody;
use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Stops an external plugin and takes it out of the registry while the backend runs: it is switched off first, its tools and
/// routes go, and its process ends. Its files, settings, tables and data folder stay; the plugin comes back with the next
/// start of the backend, or `POST /api/plugins/external/load`.
#[utoipa::path(
    post,
    path = "/api/plugins/external/unload",
    tag = "plugins",
    request_body = ExternalPluginBody,
    responses(
        (status = 204, description = "The plugin is unloaded"),
        (status = 404, description = "No such loaded external plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn unload_external_plugin(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<ExternalPluginBody>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.external_plugins.unload(&body.plugin_name, &body.plugin_subname).await?;
    Ok(StatusCode::NO_CONTENT)
}
