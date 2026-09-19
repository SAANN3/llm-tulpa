use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct PullRequest {
    /// The model tag to pull, e.g. `qwen2.5:0.5b`.
    model: String,
}

/// Pulls a model into this Ollama instance so it becomes available to switch to. Blocks
/// until the pull finishes (non-streaming) — a large model can take a while, but the
/// request's own timeout is Ollama-generous already (see `OllamaService`). Pulling a
/// model that's already present is a fast no-op.
#[utoipa::path(
    post,
    path = "/api/llm/pull",
    tag = "llm",
    request_body = PullRequest,
    responses(
        (status = 200, description = "Model pulled"),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn pull(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PullRequest>,
) -> Result<(), ErrorService> {
    state.ollama.pull_model(&body.model).await?;
    Ok(())
}
