use axum::http::StatusCode;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::services::error::ErrorService;

/// How long an issued token stays valid, in seconds (30 days). This is a local-first,
/// same-machine app — a long-lived token kept in the browser is an acceptable trade for
/// not needing a refresh flow.
const TOKEN_TTL_SECONDS: i64 = 60 * 60 * 24 * 30;

/// The JWT payload. `sub` is the user id; `role` gates owner-only actions without a second
/// database lookup on every request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i64,
    pub username: String,
    pub role: String,
    pub exp: i64,
}

/// Signs and verifies JWTs with the secret from the app config (`AppConfig::jwt_secret`).
/// Stateless — holds only the derived keys — so it can live in `AppState` and be shared.
#[derive(Clone)]
pub struct AuthService {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl AuthService {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    /// Issues a token for a user, expiring `TOKEN_TTL_SECONDS` from now.
    pub fn issue(&self, user_id: i64, username: &str, role: &str) -> Result<String, ErrorService> {
        let claims = Claims {
            sub: user_id,
            username: username.to_string(),
            role: role.to_string(),
            exp: chrono::Utc::now().timestamp() + TOKEN_TTL_SECONDS,
        };
        encode(&Header::default(), &claims, &self.encoding).map_err(|e| {
            tracing::error!("failed to sign jwt: {e}");
            ErrorService::internal("failed to issue token")
        })
    }

    /// Verifies a token and returns its claims, or a 401 if it's missing/expired/invalid.
    pub fn verify(&self, token: &str) -> Result<Claims, ErrorService> {
        decode::<Claims>(token, &self.decoding, &Validation::default())
            .map(|data| data.claims)
            .map_err(|_| ErrorService::new(StatusCode::UNAUTHORIZED, "invalid or expired token"))
    }
}
