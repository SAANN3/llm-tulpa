use std::sync::Arc;

use axum::{extract::State, Json};

use super::dto::{MemoryEntryOut, MemoryOut};
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// Everything remembered about the caller, the most recently changed first.
#[utoipa::path(
    get,
    path = "/api/memory",
    tag = "memory",
    responses((status = 200, description = "The caller's memory", body = MemoryOut)),
)]
pub async fn get_memory(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<MemoryOut>, ErrorService> {
    let entries = state.services().await?.memory_store.list(auth.id).await?;
    Ok(Json(MemoryOut { entries: entries.into_iter().map(MemoryEntryOut::from).collect() }))
}
