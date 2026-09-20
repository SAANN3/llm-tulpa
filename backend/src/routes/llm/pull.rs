use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, model_library::ModelTask},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct PullRequest {
    /// What to pull: a library name or tag (`qwen3:8b`), or a Hugging Face GGUF as
    /// `hf.co/<user>/<repo>` (optionally `:<quant>`).
    model: String,
}

/// Owner-only. Starts pulling a model into Ollama in the background and returns the task to
/// watch (`GET /api/llm/tasks`) — a multi-gigabyte pull takes far longer than a request should
/// stay open. Asking for a model that's already being pulled returns the running task.
#[utoipa::path(
    post,
    path = "/api/llm/pull",
    tag = "llm",
    request_body = PullRequest,
    responses(
        (status = 202, description = "Pull started", body = ModelTask),
        (status = 400, description = "Not a usable model name", body = crate::services::error::ErrorBody),
        (status = 403, description = "Only the owner can pull models", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn pull(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<PullRequest>,
) -> Result<(StatusCode, Json<ModelTask>), ErrorService> {
    Ok((StatusCode::ACCEPTED, Json(state.library.start_pull(&body.model)?)))
}
