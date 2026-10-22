use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DeleteMemoryQuery {
    id: i64,
}

/// Forgets one entry. Chats that already took it into their prompt keep it until their next fold.
#[utoipa::path(
    delete,
    path = "/api/memory",
    tag = "memory",
    params(DeleteMemoryQuery),
    responses(
        (status = 204, description = "Entry deleted"),
        (status = 404, description = "No such entry", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DeleteMemoryQuery>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.memory_store.delete(auth.id, query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
