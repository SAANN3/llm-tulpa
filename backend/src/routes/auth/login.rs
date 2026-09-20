use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::error::ErrorService;
use crate::services::user_store::User;
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub(crate) struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, ToSchema)]
pub struct AuthResponse {
    /// A JWT to send as `Authorization: Bearer <token>` on every other request.
    pub token: String,
    pub user: User,
}

/// Verifies credentials and returns a JWT plus the user. Public (no auth required).
#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Signed in", body = AuthResponse),
        (status = 401, description = "Unknown username or wrong password", body = crate::services::error::ErrorBody),
        (status = 503, description = "The backend has no database configured yet", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ErrorService> {
    let services = state.services().await?;
    let user = services
        .user_store
        .verify_login(&body.username, &body.password)
        .await?
        .ok_or_else(|| ErrorService::new(StatusCode::UNAUTHORIZED, "invalid username or password"))?;

    // Have the landing page's greeting ready by the time the frontend asks for it.
    services.user_cache.warm(user.id);

    let token = state.auth.issue(user.id, &user.username, &user.role)?;
    Ok(Json(AuthResponse { token, user }))
}
