use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::routes::auth::AuthUser;
use crate::services::error::ErrorService;
use crate::services::user_store::{User, ROLE_USER};
use crate::state::AppState;

fn require_owner(auth: &AuthUser) -> Result<(), ErrorService> {
    if auth.is_owner() {
        Ok(())
    } else {
        Err(ErrorService::new(StatusCode::FORBIDDEN, "only the owner can manage users"))
    }
}

/// Owner-only: lists all users.
pub async fn list_users(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<Vec<User>>, ErrorService> {
    require_owner(&auth)?;
    let services = state.services().await?;
    Ok(Json(services.user_store.list_users().await?))
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
}

/// Owner-only: creates a new (non-owner) user.
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<User>, ErrorService> {
    require_owner(&auth)?;
    let services = state.services().await?;
    let user = services.user_store.create_user(&body.username, &body.password, ROLE_USER).await?;
    Ok(Json(user))
}

#[derive(Deserialize)]
pub struct DeleteUserQuery {
    pub id: i64,
}

/// Owner-only: deletes a user (and all their data, via FK cascade). The owner can't
/// delete themselves.
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DeleteUserQuery>,
) -> Result<StatusCode, ErrorService> {
    require_owner(&auth)?;
    if query.id == auth.id {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "you can't delete your own account"));
    }
    let services = state.services().await?;
    services.user_store.delete_user(query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
