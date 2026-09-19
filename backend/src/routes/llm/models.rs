use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{services::error::ErrorService, services::llm::LocalModel, state::AppState};

/// The models installed in this Ollama instance right now — what a chat can switch to
/// without pulling first. Queried live (see `OllamaService::list_local_models`), never
/// cached, so it always reflects what's actually available. Talks only to Ollama, not the
/// database, so it works even before one is configured — but it's still behind auth, so in
/// the setup wizard the model-picker step (7) runs only after the owner exists (step 5).
#[utoipa::path(
    get,
    path = "/api/llm/models",
    tag = "llm",
    responses(
        (status = 200, description = "Installed models", body = [LocalModel]),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn models(State(state): State<Arc<AppState>>) -> Result<Json<Vec<LocalModel>>, ErrorService> {
    let models = state.ollama.list_local_models().await?;
    Ok(Json(models))
}
