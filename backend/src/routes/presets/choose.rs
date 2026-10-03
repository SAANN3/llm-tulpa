use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ChoosePresetRequest {
    model_id: i64,
    /// The preset to use for the model, or `null` to go back to the server's defaults
    preset_id: Option<i64>,
}

/// Chooses which of the caller's presets samples their calls to a model. Takes effect from the next
/// call; no model is reloaded.
#[utoipa::path(
    post,
    path = "/api/presets/choose",
    tag = "presets",
    request_body = ChoosePresetRequest,
    responses(
        (status = 204, description = "Choice saved"),
        (status = 400, description = "The preset was made for another model", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such preset or model", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn choose_preset(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ChoosePresetRequest>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.preset_store.choose(auth.id, body.model_id, body.preset_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
