use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, hf_library::HfFile},
    state::AppState,
};

#[derive(Deserialize, IntoParams)]
pub(crate) struct FilesQuery {
    /// `owner/name`
    repo: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FilesOut {
    files: Vec<HfFile>,
}

/// The GGUF files of a repository with their sizes, vision projectors marked.
#[utoipa::path(
    get,
    path = "/api/hf/files",
    tag = "hf",
    params(FilesQuery),
    responses(
        (status = 200, description = "The repository's GGUF files", body = FilesOut),
        (status = 403, description = "Gated: set a Hugging Face token in the settings", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such repository", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn files(State(state): State<Arc<AppState>>, auth: AuthUser, Query(query): Query<FilesQuery>) -> Result<Json<FilesOut>, ErrorService> {
    let token = state.services().await?.settings_store.hf_token(auth.id).await?;
    Ok(Json(FilesOut { files: state.hf.files(&query.repo, token.as_deref()).await? }))
}
