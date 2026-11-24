use crate::{routes::auth::AuthUser, services::error::{ErrorBody, ErrorService}, state::AppState};
use axum::{Json, extract::State};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;
#[derive(Serialize, ToSchema)]
pub(crate) struct ClearQueueOut {
    cancelled: u64,
}
#[utoipa::path(delete, path = "/api/blueprints/queue", tag = "blueprints",
    responses((status = 200, body = ClearQueueOut), (status = 404, body = ErrorBody)))]
pub async fn clear_queue(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<ClearQueueOut>, ErrorService> {
    Ok(Json(ClearQueueOut {
        cancelled: state
            .services()
            .await?
            .blueprints
            .clear_queue(auth.id)
            .await?,
    }))
}
