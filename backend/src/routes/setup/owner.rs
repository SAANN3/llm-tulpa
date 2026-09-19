use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::routes::auth::AuthResponse;
use crate::services::error::ErrorService;
use crate::services::user_store::ROLE_OWNER;
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub(crate) struct OwnerForm {
    username: String,
    password: String,
}

/// Public, first-run only: creates the owner account. The database itself allows at most
/// one owner (a unique index), so two simultaneous requests can't both succeed. Returns a
/// JWT so the wizard is authenticated for the remaining (per-user) steps. Any data left by a
/// previous single-user install is adopted by this account (see `services::migrate`).
#[utoipa::path(
    post,
    path = "/api/setup/owner",
    tag = "setup",
    request_body = OwnerForm,
    responses(
        (status = 200, description = "Owner created and signed in", body = AuthResponse),
        (status = 409, description = "An owner account already exists", body = crate::services::error::ErrorBody),
        (status = 503, description = "The backend has no database configured yet", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn owner(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OwnerForm>,
) -> Result<Json<AuthResponse>, ErrorService> {
    let services = state.services().await?;

    let user = services.user_store.create_user(&body.username, &body.password, ROLE_OWNER).await?;
    services.adopt_legacy_data(user.id).await?;

    let token = state.auth.issue(user.id, &user.username, &user.role)?;
    Ok(Json(AuthResponse { token, user }))
}
