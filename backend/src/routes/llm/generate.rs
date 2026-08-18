use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct GenerateRequest {
    prompt: String,
    /// Ask the model to reason before answering. Defaults to `true` when omitted.
    think: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct GenerateResponse {
    response: String,
    model: String,
    created_at: String,
    thinking: Option<String>,
}

/// Raw one-shot completion, no history and no tools — the model responds to `prompt`
/// alone.
#[utoipa::path(
    post,
    path = "/api/llm/generate",
    tag = "llm",
    request_body = GenerateRequest,
    responses(
        (status = 200, description = "Completion generated", body = GenerateResponse),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn generate(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, ErrorService> {
    let result = state.ollama.generate(body.prompt, body.think).await?;

    Ok(Json(GenerateResponse {
        response: result.response,
        model: result.model,
        created_at: result.created_at,
        thinking: result.thinking,
    }))
}
