use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// The provider a model is registered under when the request doesn't name one.
const DEFAULT_PROVIDER: &str = "llama-cpp";

#[derive(Deserialize, ToSchema)]
pub struct SetPluginModelBody {
    pub plugin_name: String,
    pub plugin_subname: String,
    /// A launch profile to run the plugin's chats on, which names its model too
    pub profile_id: Option<i64>,
    /// An installed model by name, for a provider with no launch profiles (Ollama); ignored when `profile_id` is given
    pub model: Option<String>,
    /// The provider of `model`; defaults to `llama-cpp`
    pub provider: Option<String>,
}

/// Chooses the model a plugin's chats run on, or, with neither `profile_id` nor `model`, takes the choice away. The plugin's
/// chats are moved onto it when they next get a message (an answer already being written keeps the model it started on);
/// a plugin with no choice leaves its chats on the model they have.
#[utoipa::path(
    post,
    path = "/api/plugins/model",
    tag = "plugins",
    request_body = SetPluginModelBody,
    responses(
        (status = 204, description = "Model chosen, or the choice removed"),
        (status = 400, description = "A plugin with no chats of its own, or a model that isn't installed", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such plugin or launch profile", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_plugin_model(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<SetPluginModelBody>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    let plugin = services.plugin_registry.list().await.into_iter().find(|p| p.plugin_name == body.plugin_name && p.plugin_subname == body.plugin_subname);
    match plugin {
        None => return Err(ErrorService::new(StatusCode::NOT_FOUND, "no such plugin")),
        Some(plugin) if !plugin.uses_agent => {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, "this plugin has no chats of its own, so there is no model to choose for it"));
        }
        Some(_) => {}
    }

    if let Some(profile_id) = body.profile_id {
        let profile = services.launch_store.get(profile_id).await?;
        services.model_store.set_plugin_model(&body.plugin_name, &body.plugin_subname, profile.model_id, Some(profile.id)).await?;
    } else if let Some(name) = &body.model {
        let provider = body.provider.as_deref().unwrap_or(DEFAULT_PROVIDER);
        state.require_installed_model(&services, provider, name).await?;
        let model = services.model_store.ensure(provider, name).await?;
        services.model_store.set_plugin_model(&body.plugin_name, &body.plugin_subname, model.id, None).await?;
    } else {
        services.model_store.clear_plugin_model(&body.plugin_name, &body.plugin_subname).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}
