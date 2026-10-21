use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::OwnerUser, services::tls::TlsStatus, state::AppState};

/// Owner-only. Which HTTPS certificate the frontend container serves: the one it made itself, or one uploaded here.
#[utoipa::path(
    get,
    path = "/api/tls",
    tag = "tls",
    responses((status = 200, description = "The certificates", body = TlsStatus)),
)]
pub async fn tls_status(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<TlsStatus> {
    Json(state.tls.status())
}
