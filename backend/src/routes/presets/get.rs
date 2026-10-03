use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::dto::PresetOut;
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct GetPresetsQuery {
    /// Only the presets for this model (and the any-model ones), and report the user's choice for it
    model_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PresetsOut {
    presets: Vec<PresetOut>,
    /// The preset the user has chosen for `model_id`, or `null` (the server's defaults)
    chosen: Option<i64>,
}

/// The caller's own sampling presets, by name.
#[utoipa::path(
    get,
    path = "/api/presets",
    tag = "presets",
    params(GetPresetsQuery),
    responses(
        (status = 200, description = "The caller's presets", body = PresetsOut),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_presets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<GetPresetsQuery>,
) -> Result<Json<PresetsOut>, ErrorService> {
    let services = state.services().await?;
    let presets = services.preset_store.list(auth.id, query.model_id).await?.into_iter().map(Into::into).collect();
    let chosen = match query.model_id {
        Some(model_id) => services.preset_store.chosen(auth.id, model_id).await?.map(|p| p.id),
        None => None,
    };
    Ok(Json(PresetsOut { presets, chosen }))
}
