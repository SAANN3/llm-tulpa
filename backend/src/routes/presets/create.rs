use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use super::dto::{PresetIn, PresetOut};
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// Creates a sampling preset for the caller. The name is unique per user and model.
#[utoipa::path(
    post,
    path = "/api/presets",
    tag = "presets",
    request_body = PresetIn,
    responses(
        (status = 201, description = "Preset created", body = PresetOut),
        (status = 400, description = "A value is out of range", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such model", body = crate::services::error::ErrorBody),
        (status = 409, description = "A preset with that name already exists for the model", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_preset(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<PresetIn>,
) -> Result<(StatusCode, Json<PresetOut>), ErrorService> {
    let preset = state.services().await?.preset_store.create(auth.id, body.into()).await?;
    Ok((StatusCode::CREATED, Json(preset.into())))
}
