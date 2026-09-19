use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DeleteUserQuery {
    id: i64,
}

/// Owner-only: deletes a user and all their data (chats, settings, files records) via FK
/// cascade. The owner can't delete themselves.
#[utoipa::path(
    delete,
    path = "/api/users",
    tag = "users",
    params(DeleteUserQuery),
    responses(
        (status = 204, description = "User deleted"),
        (status = 400, description = "The owner can't delete their own account", body = crate::services::error::ErrorBody),
        (status = 403, description = "Only the owner can delete users", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    OwnerUser(owner): OwnerUser,
    Query(query): Query<DeleteUserQuery>,
) -> Result<StatusCode, ErrorService> {
    if query.id == owner.id {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "you can't delete your own account"));
    }
    state.services().await?.user_store.delete_user(query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
