use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    plugins::external::manager::ExternalPluginView, routes::auth::OwnerUser, services::error::ErrorService, state::AppState,
};

/// Every external plugin the backend has seen, running or not: what it is, where it came from, whether it runs and, when it
/// does not, why. A plugin that could not be started is here with the reason, though it is not in `GET /api/plugins`.
/// Owner-only, like the plugins themselves.
#[utoipa::path(
    get,
    path = "/api/plugins/external",
    tag = "plugins",
    responses(
        (status = 200, description = "The external plugins", body = Vec<ExternalPluginView>),
    ),
)]
pub async fn list_external_plugins(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
) -> Result<Json<Vec<ExternalPluginView>>, ErrorService> {
    Ok(Json(state.services().await?.external_plugins.list().await?))
}
