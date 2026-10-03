use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DeleteProfileQuery {
    id: i64,
}

/// Owner-only. Deletes a launch profile. A chat on it keeps its model and falls back to the model's
/// default profile, so no chat is lost.
#[utoipa::path(
    delete,
    path = "/api/profiles",
    tag = "profiles",
    params(DeleteProfileQuery),
    responses(
        (status = 204, description = "Profile deleted"),
        (status = 403, description = "Only the owner can write launch profiles", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such profile", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Query(query): Query<DeleteProfileQuery>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.launch_store.delete(query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
