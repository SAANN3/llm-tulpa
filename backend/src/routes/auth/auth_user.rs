use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

use crate::services::error::ErrorService;
use crate::services::user_store::ROLE_OWNER;

/// The authenticated caller, injected into request extensions by `require_auth` and
/// pulled out by handlers as an extractor. `role` is the role the database has *now*, not
/// the one baked into the token when it was issued.
#[derive(Clone)]
pub struct AuthUser {
    pub id: i64,
    pub role: String,
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

/// An `AuthUser` that must be the owner — a handler taking this instead of `AuthUser` is
/// owner-only, rejected with 403 before its body runs.
pub struct OwnerUser(pub AuthUser);

impl<S: Send + Sync> FromRequestParts<S> for OwnerUser {
    type Rejection = ErrorService;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if user.role == ROLE_OWNER {
            Ok(Self(user))
        } else {
            Err(ErrorService::new(StatusCode::FORBIDDEN, "only the owner can do this"))
        }
    }
}
