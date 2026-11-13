use crate::{
    facade::blueprint::runs::RunSummary, routes::auth::AuthUser, services::error::{ErrorBody, ErrorService},
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
pub(crate) struct StartIn {
    expected_revision: i64,
}
#[utoipa::path(post, path = "/api/blueprints/{id}/runs", tag = "blueprints", params(("id" = i64, Path)), request_body = StartIn,
    responses((status = 202, body = RunSummary), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn start(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<StartIn>,
) -> Result<(StatusCode, Json<RunSummary>), ErrorService> {
    Ok((
        StatusCode::ACCEPTED,
        Json(
            state
                .services()
                .await?
                .blueprints
                .start(auth.id, id, body.expected_revision)
                .await?,
        ),
    ))
}
