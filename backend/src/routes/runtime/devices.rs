use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct DevicesOut {
    /// What `llama-server --list-devices` printed, errors included: a missing library shows up here.
    output: String,
}

/// The devices llama.cpp can run on, as `llama-server --list-devices` reports them — the authority
/// on whether the GPU is usable, since it is what will actually load the model.
#[utoipa::path(
    get,
    path = "/api/runtime/devices",
    tag = "runtime",
    responses(
        (status = 200, description = "The server's own device list", body = DevicesOut),
        (status = 409, description = "llama.cpp isn't installed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn devices(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Result<Json<DevicesOut>, ErrorService> {
    Ok(Json(DevicesOut { output: state.runtime.list_devices().await? }))
}
