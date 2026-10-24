use std::sync::Arc;

use axum::{extract::Query, extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::IntoParams;

use super::external_staged::StagedBody;
use crate::{plugins::external::installer::InstallTask, routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Installs a staged plugin, as a background task: its `check` runs, a plugin of kind source is built, a prebuilt binary is
/// checked against its checksum, the program is started once to see that it runs, and it is moved into place and loaded. A plugin
/// of a name that is installed replaces it only if the new version loads; otherwise the old one is put back. Answers at once with
/// the task, which `GET /api/plugins/external/tasks` follows (the build's output is in its log).
#[utoipa::path(
    post,
    path = "/api/plugins/external/install",
    tag = "plugins",
    request_body = StagedBody,
    responses(
        (status = 202, description = "The install has started", body = InstallTask),
        (status = 400, description = "It cannot be installed (the reasons are in the answer), or another install is running", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such staged plugin; it may have expired", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn install_external_plugin(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<StagedBody>) -> Result<(StatusCode, Json<InstallTask>), ErrorService> {
    let task = state.services().await?.external_plugins.start_install(&body.id).await?;
    Ok((StatusCode::ACCEPTED, Json(task)))
}

#[derive(Deserialize, IntoParams)]
pub struct TaskQuery {
    /// The id of the staged plugin that is being installed
    pub id: String,
}

/// How an install is going: what it is doing, and everything it has said so far
#[utoipa::path(
    get,
    path = "/api/plugins/external/tasks",
    tag = "plugins",
    params(TaskQuery),
    responses(
        (status = 200, description = "The install", body = InstallTask),
        (status = 404, description = "No such install", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_external_install(State(state): State<Arc<AppState>>, _owner: OwnerUser, Query(query): Query<TaskQuery>) -> Result<Json<InstallTask>, ErrorService> {
    state
        .services()
        .await?
        .external_plugins
        .task(&query.id)
        .map(Json)
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such install"))
}
