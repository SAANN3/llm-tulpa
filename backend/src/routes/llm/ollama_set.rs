use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use super::ollama_get::OllamaSettingsOut;
use crate::{
    config,
    routes::auth::OwnerUser,
    services::{error::ErrorService, llm::OllamaService},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetOllamaRequest {
    url: String,
    context_length: u64,
}

/// Owner-only. Changes where the backend reaches Ollama (applies at once) and the context window it
/// assumes (applies from the next start, as it sizes the history budget when the services are built).
/// Kept in `settings.json`.
#[utoipa::path(
    post,
    path = "/api/llm/ollama",
    tag = "llm",
    request_body = SetOllamaRequest,
    responses(
        (status = 200, description = "Saved", body = OllamaSettingsOut),
        (status = 400, description = "Not a usable address, or a context window out of range", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_ollama_settings(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<SetOllamaRequest>,
) -> Result<Json<OllamaSettingsOut>, ErrorService> {
    let url = OllamaService::normalize_url(&body.url)?;
    if !(512..=4_194_304).contains(&body.context_length) {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "the context window must be between 512 and 4194304 tokens"));
    }
    {
        let mut cfg = state.config.write().await;
        cfg.ollama.url = url.clone();
        cfg.ollama.context_length = body.context_length;
        config::save(&cfg).map_err(|e| {
            tracing::error!("could not save config: {e}");
            ErrorService::internal("the address is in use, but could not be saved to settings.json")
        })?;
    }
    state.ollama.set_base(url.clone());
    Ok(Json(OllamaSettingsOut { url, context_length: body.context_length }))
}
