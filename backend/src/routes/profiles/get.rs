use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::dto::LaunchProfileOut;
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct GetProfilesQuery {
    /// Only this model's profiles
    model_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProfilesOut {
    profiles: Vec<LaunchProfileOut>,
}

/// Every launch profile, or one model's. Profiles are shared by all users: they describe how the
/// hardware runs a model, and anyone may choose among them.
#[utoipa::path(
    get,
    path = "/api/profiles",
    tag = "profiles",
    params(GetProfilesQuery),
    responses(
        (status = 200, description = "The launch profiles", body = ProfilesOut),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_profiles(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(query): Query<GetProfilesQuery>,
) -> Result<Json<ProfilesOut>, ErrorService> {
    let services = state.services().await?;
    let profiles = services.launch_store.list(query.model_id).await?;

    let mut model_ids: Vec<i64> = profiles.iter().map(|p| p.model_id).collect();
    model_ids.sort_unstable();
    model_ids.dedup();
    let models = services.model_store.get_many(&model_ids).await?;

    let profiles = profiles
        .into_iter()
        .filter_map(|profile| {
            let model = models.get(&profile.model_id)?;
            Some(LaunchProfileOut::new(profile, model.name.clone(), model.provider.clone()))
        })
        .collect();
    Ok(Json(ProfilesOut { profiles }))
}
