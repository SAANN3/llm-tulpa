use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, hf_library::HfTask},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct DownloadRequest {
    repo: String,
    /// The file's path inside the repository
    file: String,
}

/// Owner-only. Downloads one GGUF file into the model folder (as `<repo>/<file>`) in the background,
/// resuming a partial download and checking the sha256. Watch it with `GET /api/hf/tasks`.
#[utoipa::path(
    post,
    path = "/api/hf/download",
    tag = "hf",
    request_body = DownloadRequest,
    responses(
        (status = 202, description = "Started", body = HfTask),
        (status = 403, description = "Gated: set a Hugging Face token in the settings", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn download(State(state): State<Arc<AppState>>, owner: OwnerUser, Json(body): Json<DownloadRequest>) -> Result<(StatusCode, Json<HfTask>), ErrorService> {
    let token = state.services().await?.settings_store.hf_token(owner.0.id).await?;
    Ok((StatusCode::ACCEPTED, Json(state.hf.start(&body.repo, &body.file, token).await?)))
}
