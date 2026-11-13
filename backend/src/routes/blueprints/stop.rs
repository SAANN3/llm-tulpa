use crate::{routes::auth::AuthUser, services::error::{ErrorBody, ErrorService}, state::AppState};
use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;
#[derive(Serialize, ToSchema)]
pub(crate) struct StopOut {
    run_id: Option<i64>,
}
#[utoipa::path(post, path = "/api/blueprints/queue/stop", tag = "blueprints",
    responses((status = 202, body = StopOut), (status = 404, body = ErrorBody)))]
pub async fn stop(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<StopOut>), ErrorService> {
    Ok((
        StatusCode::ACCEPTED,
        Json(StopOut {
            run_id: state.services().await?.blueprints.stop(auth.id).await?,
        }),
    ))
}
