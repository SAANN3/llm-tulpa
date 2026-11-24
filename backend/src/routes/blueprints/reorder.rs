use crate::{
    facade::blueprint::runs::QueueState, routes::auth::AuthUser, services::error::{ErrorBody, ErrorService},
    state::AppState,
};
use axum::{Json, extract::State};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReorderIn {
    run_ids: Vec<i64>,
}
#[utoipa::path(put, path = "/api/blueprints/queue", tag = "blueprints", request_body = ReorderIn,
    responses((status = 200, body = QueueState), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn reorder(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ReorderIn>,
) -> Result<Json<QueueState>, ErrorService> {
    Ok(Json(
        state
            .services()
            .await?
            .blueprints
            .reorder_queue(auth.id, &body.run_ids)
            .await?,
    ))
}
