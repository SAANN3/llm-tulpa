use crate::{
    facade::blueprint::runs::RunDetail, routes::auth::AuthUser, services::error::{ErrorBody, ErrorService},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
};
use std::sync::Arc;
#[utoipa::path(get, path = "/api/blueprints/{id}/runs/{run_id}", tag = "blueprints", params(("id" = i64, Path), ("run_id" = i64, Path)),
    responses((status = 200, body = RunDetail), (status = 404, body = ErrorBody)))]
pub async fn get_run(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((id, run_id)): Path<(i64, i64)>,
) -> Result<Json<RunDetail>, ErrorService> {
    Ok(Json(
        state
            .services()
            .await?
            .blueprints
            .get_run(auth.id, id, run_id)
            .await?,
    ))
}
