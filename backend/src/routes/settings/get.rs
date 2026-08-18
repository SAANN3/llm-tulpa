use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{services::{error::ErrorService, settings_store::Settings}, state::AppState};

/// Reads the currently persisted user settings.
#[utoipa::path(
    get,
    path = "/api/settings",
    tag = "settings",
    responses(
        (status = 200, description = "Current settings", body = Settings),
        (status = 404, description = "Settings have not been configured yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<Settings>, ErrorService> {
    let settings = state.settings_store.settings().await?;

    Ok(Json(settings))
}
