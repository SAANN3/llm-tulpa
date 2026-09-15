use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{services::error::ErrorService, services::llm::ThinkingCapability, state::AppState};

/// What the currently active model actually supports for `think` — a graded set of
/// effort levels (`Graduated`), a plain on/off toggle only (`OnOff`), or nothing at
/// all (`Unsupported`). Discovered fresh from the model's own chat template on every
/// call (see `OllamaService::thinking_capability`) — nothing here is cached or
/// persisted, so this is always correct for whichever model is actually active right
/// now, with nothing to invalidate if that changes. Meant to be called whenever a
/// caller (the composer's thinking-mode control) needs to know what to offer, not
/// once at startup — the active model can change without this app restarting.
#[utoipa::path(
    get,
    path = "/api/llm/thinking_capability",
    tag = "llm",
    responses(
        (status = 200, description = "Detected thinking capability for the active model", body = ThinkingCapability),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn thinking_capability(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ThinkingCapability>, ErrorService> {
    let result = state.ollama.thinking_capability().await?;
    Ok(Json(result))
}
