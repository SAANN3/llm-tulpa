use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub(crate) struct CatalogResponse {
    /// The raw ollama.com/library page, proxied for the client to parse — `null` when
    /// ollama.com couldn't be reached, in which case the client falls back to its own
    /// small bundled list rather than showing an error. See `OllamaService::fetch_catalog`.
    html: Option<String>,
}

/// Proxies ollama.com's public model library so the frontend can present a discoverable
/// catalog it otherwise couldn't fetch directly (CORS). Never fails the request on a
/// fetch error — an unreachable ollama.com just yields `html: null`, letting the client
/// degrade to its bundled fallback list. Parsing lives entirely on the client.
#[utoipa::path(
    get,
    path = "/api/llm/catalog",
    tag = "llm",
    responses(
        (status = 200, description = "Proxied catalog page (or null if unreachable)", body = CatalogResponse),
    ),
)]
pub async fn catalog(State(state): State<Arc<AppState>>) -> Json<CatalogResponse> {
    let html = state.ollama.fetch_catalog().await.ok();
    Json(CatalogResponse { html })
}
