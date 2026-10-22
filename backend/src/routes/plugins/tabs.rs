use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{plugins::tab::PluginTab, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// The Settings tabs the enabled plugins declare. Open to every user: a tab is for the user's own use of the plugin, while
/// switching the plugin on or off stays with the owner.
#[utoipa::path(
    get,
    path = "/api/plugins/tabs",
    tag = "plugins",
    responses((status = 200, description = "The tabs of the enabled plugins", body = Vec<PluginTab>)),
)]
pub async fn plugin_tabs(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Result<Json<Vec<PluginTab>>, ErrorService> {
    Ok(Json(state.services().await?.plugin_registry.tabs().await))
}
