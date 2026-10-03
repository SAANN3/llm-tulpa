use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use super::dto::{LaunchProfileIn, LaunchProfileOut};
use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdateProfileRequest {
    id: i64,
    #[serde(flatten)]
    settings: LaunchProfileIn,
}

/// Owner-only. Changes a launch profile; a field left out keeps its value, except `mmproj_file`
/// and `context_length`, which are replaced as sent. A running server picks the change up the next
/// time the profile is loaded.
#[utoipa::path(
    post,
    path = "/api/profiles/update",
    tag = "profiles",
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Profile updated", body = LaunchProfileOut),
        (status = 400, description = "A setting is out of range", body = crate::services::error::ErrorBody),
        (status = 403, description = "Only the owner can write launch profiles", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such profile", body = crate::services::error::ErrorBody),
        (status = 409, description = "The model already has a profile with that name", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<LaunchProfileOut>, ErrorService> {
    let services = state.services().await?;
    let current = services.launch_store.get(body.id).await?;
    let model = services
        .model_store
        .get_many(&[current.model_id])
        .await?
        .remove(&current.model_id)
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such model"))?;

    let updated = services.launch_store.update(body.id, body.settings.over(current.into())).await?;
    Ok(Json(LaunchProfileOut::new(updated, model.name, model.provider)))
}
