use std::sync::Arc;

use axum::{extract::State, Json};

use super::dto::ClearedOut;
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// Forgets everything remembered about the caller, their own entries included. Chats that already took memory into their
/// prompt keep it until their next fold.
#[utoipa::path(
    post,
    path = "/api/memory/clear",
    tag = "memory",
    responses((status = 200, description = "How many entries were removed", body = ClearedOut)),
)]
pub async fn clear_memory(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<ClearedOut>, ErrorService> {
    let removed = state.services().await?.memory_store.clear(auth.id).await?;
    Ok(Json(ClearedOut { removed }))
}
