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
    /// `downloads` (the default), `likes`, `createdAt` or `lastModified`, biggest or newest first
    sort: Option<String>,
    /// The `next_cursor` of the page before, for the one after it
    cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct SearchOut {
    repos: Vec<HfRepo>,
    /// Where the next page starts; null on the last one
    next_cursor: Option<String>,
    /// Whether the caller has a Hugging Face token set; gated repositories need one to download
    has_token: bool,
}

/// Searches Hugging Face for repositories with GGUF files, a page at a time, in the order asked for (most
/// downloaded first by default). Gated ones are flagged so the UI can hide them until the caller has set a token
/// in their settings.
#[utoipa::path(
    get,
    path = "/api/hf/search",
    tag = "hf",
    params(SearchQuery),
    responses((status = 200, description = "Matching repositories", body = SearchOut)),
)]
pub async fn search(State(state): State<Arc<AppState>>, auth: AuthUser, Query(query): Query<SearchQuery>) -> Result<Json<SearchOut>, ErrorService> {
    let token = state.services().await?.settings_store.hf_token(auth.id).await?;
    let page = state
        .hf
        .search(query.q.trim(), query.sort.as_deref().unwrap_or("downloads"), query.cursor.as_deref(), token.as_deref())
        .await?;
    Ok(Json(SearchOut { repos: page.repos, next_cursor: page.next_cursor, has_token: token.is_some() }))
}
