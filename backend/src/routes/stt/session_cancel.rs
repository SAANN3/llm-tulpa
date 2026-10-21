use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::{routes::auth::AuthUser, state::AppState};

/// Drops a live voice input without reading the rest. Does nothing for one that is gone.
#[utoipa::path(
    delete,
    path = "/api/stt/sessions/{id}",
    tag = "stt",
    params(("id" = String, Path, description = "The voice input's id")),
    responses((status = 204, description = "Dropped")),
)]
pub async fn session_cancel(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>) -> StatusCode {
    state.stt.cancel(auth.id, &id);
    StatusCode::NO_CONTENT
}
