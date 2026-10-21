use std::sync::Arc;

use axum::{extract::State, http::StatusCode};

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Owner-only. Takes the uploaded certificate away; the frontend container goes back to the one it made itself.
#[utoipa::path(
    delete,
    path = "/api/tls/certificate",
    tag = "tls",
    responses((status = 204, description = "Removed (or there was none)")),
)]
pub async fn remove_certificate(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Result<StatusCode, ErrorService> {
    state.tls.remove_custom()?;
    Ok(StatusCode::NO_CONTENT)
}
