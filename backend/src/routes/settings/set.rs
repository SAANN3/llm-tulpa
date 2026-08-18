use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::{services::{error::ErrorService, settings_store::Settings}, state::AppState};

/// Persists user settings and (re)starts the background user-cache refresh loop with the
/// new values — the loop can't run without a timezone, so writing settings for the first
/// time is what actually turns caching on. Any previously cached content is invalidated,
/// not kept, so a stale greeting generated under old settings never lingers.
#[utoipa::path(
    post,
    path = "/api/settings",
    tag = "settings",
    request_body = Settings,
    responses(
        (status = 204, description = "Settings saved"),
        (status = 400, description = "Timezone offset out of range", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Settings>,
) -> Result<StatusCode, ErrorService> {
    state.settings_store.set_settings(body).await?;
    state.user_cache.clone().start_loop().await?;

    Ok(StatusCode::NO_CONTENT)
}
