use std::sync::Arc;

use axum::{extract::Query, extract::State, Json};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{plugins::external::installer::StagedPlugin, routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub struct StagedQuery {
    pub id: String,
}

/// A staged plugin again, as `stage/upload` or `stage/git` described it
#[utoipa::path(
    get,
    path = "/api/plugins/external/stage",
    tag = "plugins",
    params(StagedQuery),
    responses(
        (status = 200, description = "The staged plugin", body = StagedPlugin),
        (status = 404, description = "No such staged plugin; it may have expired", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_staged_external(State(state): State<Arc<AppState>>, _owner: OwnerUser, Query(query): Query<StagedQuery>) -> Result<Json<StagedPlugin>, ErrorService> {
    Ok(Json(state.services().await?.external_plugins.describe(&query.id).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct StagedFileQuery {
    pub id: String,
    /// The file's path inside the plugin
    pub path: String,
}

/// The text of one file of a staged plugin, for the owner to read before installing
#[utoipa::path(
    get,
    path = "/api/plugins/external/stage/file",
    tag = "plugins",
    params(StagedFileQuery),
    responses(
        (status = 200, description = "The file's text", body = String),
        (status = 400, description = "Not a path inside the plugin, not text, or too large to show", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such staged plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_staged_external_file(State(state): State<Arc<AppState>>, _owner: OwnerUser, Query(query): Query<StagedFileQuery>) -> Result<String, ErrorService> {
    Ok(state.services().await?.external_plugins.staged_file(&query.id, &query.path).await?)
}

/// Body of the routes that name a staged plugin
#[derive(Deserialize, ToSchema)]
pub struct StagedBody {
    pub id: String,
}
