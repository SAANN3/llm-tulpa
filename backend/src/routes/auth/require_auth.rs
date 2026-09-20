use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};

use super::AuthUser;
use crate::services::error::ErrorService;
use crate::state::AppState;

/// Middleware for the protected routes: verifies the `Authorization: Bearer <jwt>` header,
/// then looks the user up so a token outlives neither its user (deleted → 401) nor their
/// role (demoted → the database's role applies, not the one in the token). Stashes an
/// `AuthUser` for handlers to extract. One primary-key read per request.
///
/// Returns `Result` (an `ErrorService` is a response too), so failures propagate with `?`
/// like they do in every handler.
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, ErrorService> {
    let unauthorized = || ErrorService::new(StatusCode::UNAUTHORIZED, "authentication required");

    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let claims = token.and_then(|t| state.auth.verify(t).ok()).ok_or_else(unauthorized)?;

    let user = state
        .services()
        .await?
        .user_store
        .get(claims.sub)
        .await?
        .ok_or_else(unauthorized)?;

    req.extensions_mut().insert(AuthUser { id: user.id, role: user.role });
    Ok(next.run(req).await)
}
