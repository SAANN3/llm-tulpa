use std::sync::Arc;

use axum::{extract::State, http::StatusCode};

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Owner-only. Records that the owner has been through the current setup, which stops the "setup
/// changed" prompt from appearing until a later release changes the wizard again.
#[utoipa::path(
    post,
    path = "/api/runtime/setup-complete",
    tag = "runtime",
    responses((status = 204, description = "Recorded")),
)]
pub async fn setup_complete(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Result<StatusCode, ErrorService> {
    state.services().await?.complete_setup().await?;
    Ok(StatusCode::NO_CONTENT)
}
