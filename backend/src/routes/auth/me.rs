use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use super::AuthUser;
use crate::services::error::ErrorService;
use crate::services::user_store::User;
use crate::state::AppState;

/// The currently-authenticated user.
#[utoipa::path(
    get,
    path = "/api/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "The signed-in user", body = User),
        (status = 401, description = "Not signed in, or the user no longer exists", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn me(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<User>, ErrorService> {
    let services = state.services().await?;
    let user = services
        .user_store
        .get(auth.id)
        .await?
        .ok_or_else(|| ErrorService::new(StatusCode::UNAUTHORIZED, "user no longer exists"))?;
    Ok(Json(user))
}
