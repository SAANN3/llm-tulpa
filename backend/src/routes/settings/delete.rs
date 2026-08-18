use std::sync::Arc;

use axum::{extract::State, http::StatusCode};

use crate::{services::error::ErrorService, state::AppState};

/// Resets user settings — mainly a dev/testing hook for exercising the "settings not
/// configured yet" UI path without going through Postgres directly. Also stops the
/// background refresh loop and drops the cached greeting, so nothing generated under
/// the old settings lingers once they're gone.
#[utoipa::path(
    delete,
    path = "/api/settings",
    tag = "settings",
    responses(
        (status = 204, description = "Settings reset"),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_settings(State(state): State<Arc<AppState>>) -> Result<StatusCode, ErrorService> {
    state.settings_store.delete_settings().await?;
    state.user_cache.stop_loop().await;

    Ok(StatusCode::NO_CONTENT)
}
