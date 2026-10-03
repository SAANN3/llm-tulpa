use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use super::dto::{LaunchProfileIn, LaunchProfileOut};
use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, launch_store::LaunchInput},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateProfileRequest {
    /// The model the profile starts
    model_id: i64,
    #[serde(flatten)]
    settings: LaunchProfileIn,
}

/// Owner-only. Adds a launch profile to a model; whatever the request leaves out takes the
/// default (the whole model on the GPU, flash attention on, an 8-bit KV cache, MTP when the file
/// has it). A profile name is unique within its model.
#[utoipa::path(
    post,
    path = "/api/profiles",
    tag = "profiles",
    request_body = CreateProfileRequest,
    responses(
        (status = 201, description = "Profile created", body = LaunchProfileOut),
        (status = 400, description = "A setting is out of range", body = crate::services::error::ErrorBody),
        (status = 403, description = "Only the owner can write launch profiles", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such model", body = crate::services::error::ErrorBody),
        (status = 409, description = "The model already has a profile with that name", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<CreateProfileRequest>,
) -> Result<(StatusCode, Json<LaunchProfileOut>), ErrorService> {
    let services = state.services().await?;
    let model = services
        .model_store
        .get_many(&[body.model_id])
        .await?
        .remove(&body.model_id)
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such model"))?;

    let profile = services.launch_store.create(model.id, body.settings.over(LaunchInput::default())).await?;
    Ok((StatusCode::CREATED, Json(LaunchProfileOut::new(profile, model.name, model.provider))))
}
