use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::{error::ErrorService, llm::LlamaServerStats}, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct RunningModelOut {
    name: String,
    /// Bytes the model takes up, when the backend says
    size: Option<u64>,
    /// Of which in GPU memory
    size_vram: Option<u64>,
    /// When the backend unloads the model if it stays idle, RFC 3339
    expires_at: Option<String>,
    context_length: Option<u64>,
}

/// What llama-server reports about itself, which only the `mtp` backend does
#[derive(Serialize, ToSchema)]
pub(crate) struct LlamaServerOut {
    model_file: Option<String>,
    slots_total: Option<u64>,
    /// Requests being worked on right now
    slots_processing: Option<u64>,
    /// Since the server started: prompt tokens evaluated and served from its cache, tokens
    /// generated, and the average speeds those add up to
    prompt_tokens_total: Option<f64>,
    prompt_tokens_cached_total: Option<f64>,
    prompt_tokens_per_second: Option<f64>,
    predicted_tokens_total: Option<f64>,
    predicted_tokens_per_second: Option<f64>,
    /// Speculative decoding: tokens drafted, and how many of them the model accepted
    draft_tokens_total: Option<f64>,
    draft_tokens_accepted_total: Option<f64>,
}

impl From<LlamaServerStats> for LlamaServerOut {
    fn from(stats: LlamaServerStats) -> Self {
        Self {
            model_file: stats.model_file,
            slots_total: stats.slots_total,
            slots_processing: stats.slots_processing,
            prompt_tokens_total: stats.prompt_tokens_total,
            prompt_tokens_cached_total: stats.prompt_tokens_cached_total,
            prompt_tokens_per_second: stats.prompt_tokens_per_second,
            predicted_tokens_total: stats.predicted_tokens_total,
            predicted_tokens_per_second: stats.predicted_tokens_per_second,
            draft_tokens_total: stats.draft_tokens_total,
            draft_tokens_accepted_total: stats.draft_tokens_accepted_total,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ServerResponse {
    /// Whether the model backend answered at all
    reachable: bool,
    /// For the managed llama.cpp when it isn't answering: `stopped`, `starting`, `failed` or `not_installed`
    state: Option<String>,
    /// Why, when it failed or is missing
    detail: Option<String>,
    /// The models it has loaded right now (empty when idle and unloaded)
    models: Vec<RunningModelOut>,
    llama_server: Option<LlamaServerOut>,
}

/// What the model backend is running right now, from its provider — Ollama's own `/api/ps`, or
/// llama.cpp's own server.
#[utoipa::path(
    get,
    path = "/api/stats/server",
    tag = "stats",
    responses(
        (status = 200, description = "The backend's current state", body = ServerResponse),
    ),
)]
pub async fn server(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Result<Json<ServerResponse>, ErrorService> {
    let services = state.services().await?;
    let snapshot = services.stats.server().await;

    Ok(Json(ServerResponse {
        reachable: snapshot.reachable,
        state: snapshot.state,
        detail: snapshot.detail,
        models: snapshot
            .models
            .into_iter()
            .map(|model| RunningModelOut {
                name: model.name,
                size: model.size,
                size_vram: model.size_vram,
                expires_at: model.expires_at,
                context_length: model.context_length,
            })
            .collect(),
        llama_server: snapshot.llama_server.map(LlamaServerOut::from),
    }))
}
