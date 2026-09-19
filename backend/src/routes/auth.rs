use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::services::error::{ErrorBody, ErrorService};
use crate::services::user_store::{User, ROLE_OWNER};
use crate::state::AppState;

/// The authenticated caller, injected into request extensions by `require_auth` and
/// pulled out by handlers as an extractor.
#[derive(Clone)]
pub struct AuthUser {
    pub id: i64,
    pub role: String,
}

impl AuthUser {
    pub fn is_owner(&self) -> bool {
        self.role == ROLE_OWNER
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ErrorService;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| ErrorService::new(StatusCode::UNAUTHORIZED, "authentication required"))
    }
}

/// Middleware for the protected routes: verifies the `Authorization: Bearer <jwt>` header
/// and stashes an `AuthUser` for handlers to extract. 401s when the token is absent or
/// invalid.
pub async fn require_auth(State(state): State<Arc<AppState>>, mut req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    match token.and_then(|t| state.auth.verify(t).ok()) {
        Some(claims) => {
            req.extensions_mut().insert(AuthUser { id: claims.sub, role: claims.role });
            next.run(req).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody { error: "authentication required".to_string() }),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

/// Verifies credentials and returns a JWT plus the user. Public (no auth required).
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

    let token = state.auth.issue(user.id, &user.username, &user.role)?;
    Ok(Json(AuthResponse { token, user }))
}

/// The currently-authenticated user (protected).
pub async fn me(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<User>, ErrorService> {
    let services = state.services().await?;
    let user = services
        .user_store
        .get(auth.id)
        .await?
        .ok_or_else(|| ErrorService::new(StatusCode::UNAUTHORIZED, "user no longer exists"))?;
    Ok(Json(user))
}
