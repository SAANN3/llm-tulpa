use crate::{
    facade::blueprint::runs::RunSummary, routes::auth::AuthUser, services::error::{ErrorBody, ErrorService},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};
#[derive(Deserialize, IntoParams)]
pub(crate) struct HistoryQuery {
    #[serde(default)]
    offset: u64,
    #[serde(default = "default_limit")]
    limit: u64,
}
fn default_limit() -> u64 {
    20
}
#[derive(Serialize, ToSchema)]
pub(crate) struct HistoryOut {
    items: Vec<RunSummary>,
    total: u64,
    offset: u64,
    limit: u64,
}
#[utoipa::path(get, path = "/api/blueprints/{id}/runs", tag = "blueprints", params(("id" = i64, Path), HistoryQuery),
    responses((status = 200, body = HistoryOut), (status = 404, body = ErrorBody)))]
pub async fn history(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryOut>, ErrorService> {
    let (items, total) = state
        .services()
        .await?
        .blueprints
        .list_runs(auth.id, id, query.offset, query.limit)
        .await?;
    Ok(Json(HistoryOut {
        items,
        total,
        offset: query.offset,
        limit: query.limit,
    }))
}
