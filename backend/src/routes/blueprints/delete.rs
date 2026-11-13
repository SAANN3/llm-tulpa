use crate::{routes::auth::AuthUser, services::error::{ErrorBody, ErrorService}, state::AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

#[derive(Deserialize, IntoParams, ToSchema)]
pub(crate) struct DeleteQuery {
    pub expected_revision: i64,
}

#[utoipa::path(delete, path = "/api/blueprints/{id}", tag = "blueprints", params(("id" = i64, Path), DeleteQuery),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn delete(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<DeleteQuery>,
) -> Result<StatusCode, ErrorService> {
    state
        .services()
        .await?
        .blueprints
        .delete(auth.id, id, query.expected_revision)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
