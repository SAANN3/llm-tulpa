use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, stt::SttTask},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct InstallRequest {
    /// An id from `GET /api/stt`
    model: String,
}

/// Owner-only. Downloads a speech-to-text model and unpacks it, in the background; watch it with `GET /api/stt`. The
/// download is checked against the sha256 the backend was built with.
#[utoipa::path(
    post,
    path = "/api/stt/models",
    tag = "stt",
    request_body = InstallRequest,
    responses(
        (status = 202, description = "Started", body = SttTask),
        (status = 404, description = "No such model", body = crate::services::error::ErrorBody),
        (status = 409, description = "Already installed, or another install is running", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn install(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<InstallRequest>) -> Result<(StatusCode, Json<SttTask>), ErrorService> {
    Ok((StatusCode::ACCEPTED, Json(state.stt.start_install(&body.model)?)))
}
