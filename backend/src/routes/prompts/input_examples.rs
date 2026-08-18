use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct InputExampleOut {
    text: String,
}

/// A short, randomly-picked placeholder string for the chat composer's empty input.
/// Served from `UserCacheService`'s background-refreshed cache, not generated live on
/// this request — errs only if user settings (needed for a timezone) haven't been
/// configured yet.
#[utoipa::path(
    post,
    path = "/api/prompts/input_examples",
    tag = "prompts",
    responses(
        (status = 200, description = "Placeholder text", body = InputExampleOut),
        (status = 409, description = "User settings have not been configured yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn input_examples(State(state): State<Arc<AppState>>) -> Result<Json<InputExampleOut>, ErrorService> {
    let text = state.user_cache.clone().input_examples().await?;

    Ok(Json(InputExampleOut { text }))
}
