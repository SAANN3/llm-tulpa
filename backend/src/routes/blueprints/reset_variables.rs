use std::sync::Arc;

use axum::{extract::{Path, Query, State}, http::StatusCode};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::{ErrorBody, ErrorService}, state::AppState};

#[derive(Deserialize, IntoParams, ToSchema)]
pub(crate) struct ResetVariablesQuery {
    pub expected_revision: i64,
    pub node_id: Option<String>,
}

#[utoipa::path(delete, path = "/api/blueprints/{id}/variables", tag = "blueprints",
    params(("id" = i64, Path), ResetVariablesQuery),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn reset_variables(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<ResetVariablesQuery>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.blueprints.reset_variables(auth.id, id, query.expected_revision, query.node_id.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}
