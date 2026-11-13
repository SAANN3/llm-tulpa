use crate::{
    facade::agent::Allowance,
    routes::auth::AuthUser,
    services::error::{ErrorBody, ErrorService},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnswerIn {
    request_id: String,
    allowance: Allowance,
}
#[utoipa::path(post, path = "/api/blueprints/{id}/runs/{run_id}/answer", tag = "blueprints",
    params(("id" = i64, Path), ("run_id" = i64, Path)), request_body = AnswerIn,
    responses((status = 202), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn answer(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path((id, run_id)): Path<(i64, i64)>,
    Json(body): Json<AnswerIn>,
) -> Result<StatusCode, ErrorService> {
    state
        .services()
        .await?
        .blueprints
        .answer_tool(auth.id, id, run_id, &body.request_id, body.allowance)
        .await?;
    Ok(StatusCode::ACCEPTED)
}
