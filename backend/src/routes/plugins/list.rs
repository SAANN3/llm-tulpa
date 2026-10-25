use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    plugins::registry::{PluginInfo, PluginModelInfo}, routes::auth::OwnerUser, services::error::ErrorService, state::AppState,
};

/// Every registered plugin, enabled and disabled alike — what the frontend's plugin
/// list page renders. Owner-only: plugins run under the owner's account, and each entry
/// carries its settings (bot tokens included). A plugin appears here as soon as its builder is registered at
/// startup, even before it's ever been configured (see `PluginInfo::settings`).
#[utoipa::path(
    get,
    path = "/api/plugins",
    tag = "plugins",
    responses(
        (status = 200, description = "Every registered plugin", body = Vec<PluginInfo>),
    ),
)]
pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
) -> Result<Json<Vec<PluginInfo>>, ErrorService> {
    let services = state.services().await?;
    let mut plugins = services.plugin_registry.list().await;
    let mut models = services.model_store.plugin_models().await?;
    for plugin in &mut plugins {
        plugin.model = models.remove(&(plugin.plugin_name.clone(), plugin.plugin_subname.clone())).map(|choice| PluginModelInfo {
            provider: choice.model.provider,
            model: choice.model.name,
            launch_profile_id: choice.launch_profile_id,
        });
    }
    Ok(Json(plugins))
}
