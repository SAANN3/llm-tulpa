use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::OwnerUser, services::model_library::Catalog, state::AppState};

/// Owner-only. Ollama's public model library as structured data — what the model picker offers
/// to pull. Parsed on the backend from ollama.com's page (see `model_library::parse_catalog`);
/// never fails the request on a fetch error — when ollama.com can't be reached, `live` is
/// `false` and `models` is a short built-in list.
#[utoipa::path(
    get,
    path = "/api/llm/catalog",
    tag = "llm",
    responses(
        (status = 200, description = "The model library (or a built-in fallback)", body = Catalog),
        (status = 403, description = "Only the owner can browse the catalog", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn catalog(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<Catalog> {
    Json(state.library.catalog().await)
}
