use std::{collections::BTreeMap, sync::Arc};

use axum::{Json, extract::{Path, State}};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    facade::blueprint::definition::Literal,
    routes::auth::AuthUser,
    services::error::{ErrorBody, ErrorService},
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub(crate) struct VariablesOut {
    pub values: BTreeMap<String, Literal>,
}

#[utoipa::path(get, path = "/api/blueprints/{id}/variables", tag = "blueprints", params(("id" = i64, Path)),
    responses((status = 200, body = VariablesOut), (status = 404, body = ErrorBody)))]
pub async fn variables(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<VariablesOut>, ErrorService> {
    let values = state.services().await?.blueprints.variables(auth.id, id).await?;
    Ok(Json(VariablesOut { values }))
}
