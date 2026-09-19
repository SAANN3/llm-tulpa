use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{plugins::registry::PluginInfo, services::error::ErrorService, state::AppState};

/// Every registered plugin, enabled and disabled alike — what the frontend's plugin
/// list page renders. A plugin appears here as soon as its builder is registered at
/// startup, even before it's ever been configured (see `PluginInfo::settings`).
#[utoipa::path(
    get,
    path = "/api/plugins",
    tag = "plugins",
    responses(
        (status = 200, description = "Every registered plugin", body = Vec<PluginInfo>),
    ),
)]
pub async fn list_plugins(State(state): State<Arc<AppState>>) -> Result<Json<Vec<PluginInfo>>, ErrorService> {
    Ok(Json(state.services().await?.plugin_registry.list().await))
}
