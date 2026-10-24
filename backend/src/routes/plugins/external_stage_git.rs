use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{plugins::external::installer::StagedPlugin, routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct StageGitBody {
    /// `https://...`, `ssh://...` or `user@host:path`
    pub url: String,
    /// A branch or a tag; the repository's default branch when left out
    pub git_ref: Option<String>,
}

/// First step of installing a plugin from git: the repository is fetched (the latest commit of the branch or tag, without its
/// history) into a staging area and described, like `stage/upload` does for a file. Needs `git` on the machine the backend
/// runs on. The commit that was fetched is in the answer's `source.commit`.
#[utoipa::path(
    post,
    path = "/api/plugins/external/stage/git",
    tag = "plugins",
    request_body = StageGitBody,
    responses(
        (status = 200, description = "The plugin, fetched and described", body = StagedPlugin),
        (status = 400, description = "Not an address git may fetch, git failed, or what it fetched is not a plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn stage_external_git(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<StageGitBody>) -> Result<Json<StagedPlugin>, ErrorService> {
    Ok(Json(state.services().await?.external_plugins.stage_git(body.url.trim(), body.git_ref.as_deref().map(str::trim).filter(|r| !r.is_empty())).await?))
}
