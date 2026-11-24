use crate::{routes::auth::AuthUser, services::error::{ErrorBody, ErrorService}, state::AppState};
use axum::{extract::{Path, State}, http::StatusCode};
use std::sync::Arc;

#[utoipa::path(delete, path = "/api/blueprints/queue/{run_id}", tag = "blueprints", params(("run_id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn cancel_queued(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(run_id): Path<i64>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.blueprints.cancel_queued(auth.id, run_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
