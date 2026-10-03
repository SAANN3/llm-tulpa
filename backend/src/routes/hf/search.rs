use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, hf_library::HfRepo},
    state::AppState,
};

#[derive(Deserialize, IntoParams)]
pub(crate) struct SearchQuery {
    q: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SearchOut {
    repos: Vec<HfRepo>,
    /// Whether the caller has a Hugging Face token set; gated repositories need one to download
    has_token: bool,
}

/// Searches Hugging Face for repositories with GGUF files, most downloaded first. Gated ones are
/// flagged so the UI can hide them until the caller has set a token in their settings.
#[utoipa::path(
    get,
    path = "/api/hf/search",
    tag = "hf",
    params(SearchQuery),
    responses((status = 200, description = "Matching repositories", body = SearchOut)),
)]
pub async fn search(State(state): State<Arc<AppState>>, auth: AuthUser, Query(query): Query<SearchQuery>) -> Result<Json<SearchOut>, ErrorService> {
    let token = state.services().await?.settings_store.hf_token(auth.id).await?;
    let repos = state.hf.search(query.q.trim(), token.as_deref()).await?;
    Ok(Json(SearchOut { repos, has_token: token.is_some() }))
}
