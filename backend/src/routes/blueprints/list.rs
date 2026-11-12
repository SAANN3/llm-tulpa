use super::get::BlueprintOut;
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

#[derive(Default, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Sort {
    #[default]
    Created,
    Updated,
}

#[derive(Deserialize, IntoParams, ToSchema)]
pub(crate) struct ListQuery {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub sort: Sort,
}
fn default_limit() -> u64 {
    20
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BlueprintListOut {
    pub items: Vec<BlueprintOut>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

#[utoipa::path(get, path = "/api/blueprints", tag = "blueprints", params(ListQuery),
    responses((status = 200, body = BlueprintListOut), (status = 400, body = crate::services::error::ErrorBody)))]
pub async fn list(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<BlueprintListOut>, ErrorService> {
    let (items, total) = state
        .services()
        .await?
        .blueprints
        .list(
            auth.id,
            query.offset,
            query.limit,
            &query.q,
            matches!(query.sort, Sort::Updated),
        )
        .await?;
    Ok(Json(BlueprintListOut {
        items: items.into_iter().map(Into::into).collect(),
        total,
        offset: query.offset,
        limit: query.limit,
    }))
}
