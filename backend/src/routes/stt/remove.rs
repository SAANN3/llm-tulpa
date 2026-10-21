use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize)]
pub(crate) struct ModelQuery {
    model: String,
}

/// Owner-only. Removes an installed speech-to-text model and its files.
#[utoipa::path(
    delete,
    path = "/api/stt/models",
    tag = "stt",
    params(("model" = String, Query, description = "The model's id")),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "No such model", body = crate::services::error::ErrorBody),
        (status = 409, description = "It is being installed or in use", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn remove(State(state): State<Arc<AppState>>, _owner: OwnerUser, Query(query): Query<ModelQuery>) -> Result<StatusCode, ErrorService> {
    state.stt.remove(&query.model)?;
    Ok(StatusCode::NO_CONTENT)
}
