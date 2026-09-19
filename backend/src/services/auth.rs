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

    /// Hashes a password with bcrypt. bcrypt is deliberately slow (tens of milliseconds); run
    /// inline it would stall every other request on the async worker thread for that long, so
    /// it goes to the blocking pool.
    pub async fn hash_password(password: &str) -> Result<String, AuthErrors> {
        let password = password.to_string();
        tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
            .await
            .map_err(|_| AuthErrors::Join)?
            .map_err(AuthErrors::Hash)
    }

    /// Whether `password` matches a hash from `hash_password`, on the blocking pool for the same
    /// reason.
    pub async fn verify_password(password: &str, hash: String) -> Result<bool, AuthErrors> {
        let password = password.to_string();
        tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash))
            .await
            .map_err(|_| AuthErrors::Join)?
            .map_err(AuthErrors::Hash)
    }

    /// A well-formed hash nobody's password matches, verified against when a login names an
    /// unknown user so that case takes as long as a wrong password does.
    pub fn dummy_hash() -> &'static str {
        static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        HASH.get_or_init(|| bcrypt::hash("no such user", bcrypt::DEFAULT_COST).expect("bcrypt hashing a constant"))
    }

    /// Verifies a token and returns its claims, or a 401 if it's missing/expired/invalid.
    pub fn verify(&self, token: &str) -> Result<Claims, ErrorService> {
        decode::<Claims>(token, &self.decoding, &Validation::default())
            .map(|data| data.claims)
            .map_err(|_| ErrorService::new(StatusCode::UNAUTHORIZED, "invalid or expired token"))
    }
}

#[derive(Debug)]
pub enum AuthErrors {
    Hash(bcrypt::BcryptError),
    /// The blocking-pool task running bcrypt panicked or was cancelled.
    Join,
}

impl From<AuthErrors> for ErrorService {
    fn from(err: AuthErrors) -> Self {
        match err {
            AuthErrors::Hash(e) => tracing::error!("password hashing failed: {e}"),
            AuthErrors::Join => tracing::error!("password hashing task failed"),
        }
        ErrorService::internal("password hashing failed")
    }
}
