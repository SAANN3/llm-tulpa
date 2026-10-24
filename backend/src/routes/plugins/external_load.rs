use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    plugins::external::manager::ExternalPluginView, routes::auth::OwnerUser, services::error::ErrorService, state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub struct ExternalPluginBody {
    pub plugin_name: String,
    pub plugin_subname: String,
}

/// Loads the external plugin in `<external_plugins_dir>/<plugin_name>/<plugin_subname>` while the backend runs, as if it had been
/// there at start-up: its `check` runs, its process starts, its migrations are applied and it is registered (off, unless it
/// is on by default or was switched on before). A plugin that cannot start is recorded with the reason, which is also the
/// answer (400).
#[utoipa::path(
    post,
    path = "/api/plugins/external/load",
    tag = "plugins",
    request_body = ExternalPluginBody,
    responses(
        (status = 200, description = "The plugin is loaded", body = ExternalPluginView),
        (status = 400, description = "It could not be started, or is already loaded, or is a built-in plugin", body = crate::services::error::ErrorBody),
        (status = 404, description = "There is no plugin.toml in that folder", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn load_external_plugin(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<ExternalPluginBody>,
) -> Result<Json<ExternalPluginView>, ErrorService> {
    Ok(Json(state.services().await?.external_plugins.load(&body.plugin_name, &body.plugin_subname).await?))
}
