use crate::{
    facade::blueprint::{definition::Workflow, validation::report::ValidationReport},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};
use axum::{Json, extract::State};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ValidateIn {
    pub workflow: Workflow,
}

/// Checks an editor's unsaved document without persisting it or calling any tool/provider.
#[utoipa::path(post, path = "/api/blueprints/validate", tag = "blueprints", request_body = ValidateIn,
    responses((status = 200, body = ValidationReport)))]
pub async fn validate(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ValidateIn>,
) -> Result<Json<ValidationReport>, ErrorService> {
    Ok(Json(
        state
            .services()
            .await?
            .blueprints
            .validate(auth.id, &body.workflow)
            .await?,
    ))
}
