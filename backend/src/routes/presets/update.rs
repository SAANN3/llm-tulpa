use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use super::dto::{PresetIn, PresetOut};
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpdatePresetRequest {
    id: i64,
    #[serde(flatten)]
    preset: PresetIn,
}

/// Replaces one of the caller's presets as a whole.
#[utoipa::path(
    post,
    path = "/api/presets/update",
    tag = "presets",
    request_body = UpdatePresetRequest,
    responses(
        (status = 200, description = "Preset updated", body = PresetOut),
        (status = 400, description = "A value is out of range", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such preset", body = crate::services::error::ErrorBody),
        (status = 409, description = "A preset with that name already exists for the model", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn update_preset(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<UpdatePresetRequest>,
) -> Result<Json<PresetOut>, ErrorService> {
    let preset = state.services().await?.preset_store.update(auth.id, body.id, body.preset.into()).await?;
    Ok(Json(preset.into()))
}
