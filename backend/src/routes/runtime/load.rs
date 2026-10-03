use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, llama_runtime::RuntimeStatus},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct LoadRequest {
    profile_id: i64,
}

/// Loads a launch profile into the model server and returns once it is ready (which for a big model
/// takes a while — watch `model_state` events meanwhile). Switching away from another profile is
/// refused with 423 while somebody has a turn running on it.
#[utoipa::path(
    post,
    path = "/api/runtime/load",
    tag = "runtime",
    request_body = LoadRequest,
    responses(
        (status = 200, description = "Loaded", body = RuntimeStatus),
        (status = 404, description = "No such profile", body = crate::services::error::ErrorBody),
        (status = 409, description = "llama.cpp isn't installed, or the server is an external one", body = crate::services::error::ErrorBody),
        (status = 423, description = "Another user has a turn running on the loaded model", body = crate::services::error::ErrorBody),
        (status = 502, description = "The model failed to load; the message says why", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn load(State(state): State<Arc<AppState>>, auth: AuthUser, Json(body): Json<LoadRequest>) -> Result<Json<RuntimeStatus>, ErrorService> {
    let request = state.services().await?.launches.request_for_profile(auth.id, body.profile_id).await?;
    state.runtime.load(&request).await?;
    Ok(Json(state.runtime.status()))
}
