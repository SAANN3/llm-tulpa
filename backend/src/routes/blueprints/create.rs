use super::get::BlueprintOut;
use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;

use crate::{
    facade::blueprint::definition::{Schedule, Workflow},
    routes::auth::AuthUser,
    services::error::{ErrorBody, ErrorService},
    state::AppState,
};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateBlueprintIn {
    pub name: String,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default)]
    pub workflow: Workflow,
}

#[utoipa::path(post, path = "/api/blueprints", tag = "blueprints", request_body = CreateBlueprintIn,
    responses((status = 201, body = BlueprintOut), (status = 400, body = ErrorBody)))]
pub async fn create(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateBlueprintIn>,
) -> Result<(StatusCode, Json<BlueprintOut>), ErrorService> {
    let record = state
        .services()
        .await?
        .blueprints
        .create(auth.id, &body.name, &body.schedule, &body.workflow)
        .await?;
    Ok((StatusCode::CREATED, Json(record.into())))
}
