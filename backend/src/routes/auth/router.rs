use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{login::*, me::*};

/// The auth routes that need a valid token.
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/me", get(me))
}

/// The auth routes that can't require one — signing in is how a caller gets a token.
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/login", post(login))
}

#[derive(OpenApi)]
#[openapi(
    paths(login, me),
    components(schemas(
        LoginRequest,
        AuthResponse,
        crate::services::user_store::User,
    )),
)]
pub struct ApiDoc;
