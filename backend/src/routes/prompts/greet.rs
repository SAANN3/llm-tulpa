use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{facade::prompt::GreetOut, services::error::ErrorService, state::AppState};

/// A short, lively greeting for the "no chat open yet" landing page — never a
/// predefined string, and nudged to reference the time of day rather than state it
/// outright. Served from `UserCacheService`'s background-refreshed cache, not generated
/// live on this request — errs only if user settings (needed for a timezone) haven't
/// been configured yet.
#[utoipa::path(
    post,
    path = "/api/prompts/greet",
    tag = "prompts",
    responses(
        (status = 200, description = "Greeting generated", body = GreetOut),
        (status = 409, description = "User settings have not been configured yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn greet(State(state): State<Arc<AppState>>) -> Result<Json<GreetOut>, ErrorService> {
    let result = state.user_cache.clone().greet().await?;

    Ok(Json(result))
}
