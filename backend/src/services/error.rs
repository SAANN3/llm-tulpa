use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

/// The one error type route handlers return. Every service defines its own error enum
/// (e.g. `OllamaErrors`) and converts into this via `From`, so handlers can propagate
/// with `?` without knowing which service failed — `IntoResponse` below turns it into
/// the actual HTTP response.
pub struct ErrorService {
    pub http_code: StatusCode,
    pub message: Option<String>,
}

impl ErrorService {
    pub fn new(http_code: StatusCode, message: impl Into<String>) -> Self {
        Self {
            http_code,
            message: Some(message.into()),
        }
    }

    /// Shorthand for the common case of an unexpected/internal failure — always 500.
    /// Use `new` directly when the failure maps to a more specific status (e.g. a
    /// bad-gateway when an upstream service errors).
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

/// Lets `ErrorService` be returned directly as a handler's `Err` type — axum calls this
/// to turn it into an actual HTTP response, so no handler has to manually build one.
impl IntoResponse for ErrorService {
    fn into_response(self) -> Response {
        let error = self.message.unwrap_or_else(|| "internal error".to_string());
        (self.http_code, Json(ErrorBody { error })).into_response()
    }
}

/// The JSON body every error response carries, whatever the status code — shared across
/// route handlers' `#[utoipa::path]` error responses in the generated API docs, since
/// they'd otherwise each have to restate `{"error": string}` themselves.
#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}
