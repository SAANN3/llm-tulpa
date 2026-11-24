use crate::{
    facade::blueprint::runs::QueueState, routes::auth::AuthUser, services::error::{ErrorBody, ErrorService},
    state::AppState,
};
use axum::{Json, extract::State};
use std::sync::Arc;
#[utoipa::path(get, path = "/api/blueprints/queue", tag = "blueprints",
    responses((status = 200, body = QueueState), (status = 404, body = ErrorBody)))]
pub async fn queue(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<QueueState>, ErrorService> {
    Ok(Json(
        state.services().await?.blueprints.queue(auth.id).await?,
    ))
}
