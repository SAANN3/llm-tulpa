use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::OwnerUser, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct OllamaSettingsOut {
    /// Where Ollama answers
    pub(crate) url: String,
    /// The context window Ollama runs the model with: must match Ollama's own setting; applies from the next start
    pub(crate) context_length: u64,
}

/// Owner-only. Where the backend reaches Ollama (the optional second model provider) and the context
/// window it assumes Ollama gives its models.
#[utoipa::path(
    get,
    path = "/api/llm/ollama",
    tag = "llm",
    responses((status = 200, description = "The Ollama settings", body = OllamaSettingsOut)),
)]
pub async fn get_ollama_settings(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<OllamaSettingsOut> {
    let config = state.config.read().await;
    Json(OllamaSettingsOut { url: state.ollama.base(), context_length: config.ollama.context_length })
}
