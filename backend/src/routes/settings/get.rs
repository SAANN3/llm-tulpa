use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, settings_store::Settings},
    state::AppState,
};

/// Reads the authenticated user's settings.
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
pub async fn get_settings(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<Settings>, ErrorService> {
    let settings = state.services().await?.settings_store.settings(auth.id).await?;

    Ok(Json(settings))
}
