use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{
        error::ErrorService,
        user_store::{User, ROLE_USER},
    },
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateUserRequest {
    username: String,
    password: String,
}

/// Owner-only: creates a new (non-owner) user.
#[utoipa::path(
    post,
    path = "/api/users",
    tag = "users",
    request_body = CreateUserRequest,
    responses(
        (status = 200, description = "The created user", body = User),
        (status = 403, description = "Only the owner can create users", body = crate::services::error::ErrorBody),
        (status = 409, description = "A user with that name already exists", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<User>, ErrorService> {
    let services = state.services().await?;
    let user = services.user_store.create_user(&body.username, &body.password, ROLE_USER).await?;
    Ok(Json(user))
}
