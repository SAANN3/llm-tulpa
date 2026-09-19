use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, user_store::User},
    state::AppState,
};

/// Owner-only: lists all users, oldest first.
#[utoipa::path(
    get,
    path = "/api/users",
    tag = "users",
    responses(
        (status = 200, description = "Every user", body = [User]),
        (status = 403, description = "Only the owner can list users", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn list_users(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Result<Json<Vec<User>>, ErrorService> {
    Ok(Json(state.services().await?.user_store.list_users().await?))
}
