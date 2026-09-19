use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, settings_store::SettingsUpdate},
    state::AppState,
};

/// Applies a partial update to the authenticated user's settings — only the provided
/// fields are written. Used both by the settings page (a full save) and the setup
/// wizard's incremental per-step saves.
#[utoipa::path(
    post,
    path = "/api/settings",
    tag = "settings",
    request_body = SettingsUpdate,
    responses(
        (status = 204, description = "Settings saved"),
        (status = 400, description = "Timezone offset out of range", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SettingsUpdate>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.settings_store.update(auth.id, body).await?;

    Ok(StatusCode::NO_CONTENT)
}
