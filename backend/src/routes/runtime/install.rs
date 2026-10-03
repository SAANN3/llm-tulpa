use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::{
    routes::auth::OwnerUser,
    services::{
        error::ErrorService,
        llama_install::{InstallRequest, InstallTask},
    },
    state::AppState,
};

/// Owner-only. Installs llama.cpp as a background task and returns at once; watch it with
/// `GET /api/runtime/install`. `pinned` is the release this version was tested with and is checked
/// against its published sha256; `latest` is the newest release and may break; `custom` uses the
/// `llama-server` binary at `custom_path`. A running model server is stopped first.
#[utoipa::path(
    post,
    path = "/api/runtime/install",
    tag = "runtime",
    request_body = InstallRequest,
    responses(
        (status = 202, description = "Started", body = InstallTask),
        (status = 400, description = "No such build for this system, or no binary named", body = crate::services::error::ErrorBody),
        (status = 409, description = "An install is already running", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn install(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<InstallRequest>,
) -> Result<(StatusCode, Json<InstallTask>), ErrorService> {
    Ok((StatusCode::ACCEPTED, Json(state.installer.start(body)?)))
}
