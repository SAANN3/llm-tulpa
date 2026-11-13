use axum::{
    Json,
    extract::{Path, State},
};
use std::sync::Arc;

use crate::{
    facade::blueprint::definition::{Schedule, Workflow},
    routes::auth::AuthUser,
    services::{blueprint_store::BlueprintRecord, error::{ErrorBody, ErrorService}},
    state::AppState,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub(crate) struct BlueprintOut {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub revision: i64,
    pub schedule: Schedule,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl From<BlueprintRecord> for BlueprintOut {
    fn from(r: BlueprintRecord) -> Self {
        Self {
            id: r.id,
            user_id: r.user_id,
            name: r.name,
            revision: r.revision,
            schedule: r.schedule,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BlueprintDocumentOut {
    pub blueprint: BlueprintOut,
    pub workflow: Workflow,
}

#[utoipa::path(get, path = "/api/blueprints/{id}", tag = "blueprints", params(("id" = i64, Path)),
    responses((status = 200, body = BlueprintDocumentOut), (status = 404, body = ErrorBody)))]
pub async fn get(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<BlueprintDocumentOut>, ErrorService> {
    let (record, workflow) = state.services().await?.blueprints.get(auth.id, id).await?;
    Ok(Json(BlueprintDocumentOut {
        blueprint: record.into(),
        workflow,
    }))
}
