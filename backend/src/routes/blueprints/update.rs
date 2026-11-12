use super::get::BlueprintOut;
use axum::{
    Json,
    extract::{Path, State},
};
use std::sync::Arc;

use crate::{
    facade::blueprint::definition::{Schedule, Workflow},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateBlueprintIn {
    pub expected_revision: i64,
    pub name: String,
    pub schedule: Schedule,
    /// Omitted or null preserves the existing workflow file.
    pub workflow: Option<Workflow>,
}

#[utoipa::path(put, path = "/api/blueprints/{id}", tag = "blueprints", params(("id" = i64, Path)), request_body = UpdateBlueprintIn,
    responses((status = 200, body = BlueprintOut), (status = 400, body = crate::services::error::ErrorBody),
        (status = 404, body = crate::services::error::ErrorBody), (status = 409, body = crate::services::error::ErrorBody)))]
pub async fn update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBlueprintIn>,
) -> Result<Json<BlueprintOut>, ErrorService> {
    let record = state
        .services()
        .await?
        .blueprints
        .update(
            auth.id,
            id,
            body.expected_revision,
            &body.name,
            &body.schedule,
            body.workflow.as_ref(),
        )
        .await?;
    Ok(Json(record.into()))
}
