use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DeletePresetQuery {
    id: i64,
}

/// Deletes one of the caller's presets; if it was their choice for a model, that choice goes with it.
#[utoipa::path(
    delete,
    path = "/api/presets",
    tag = "presets",
    params(DeletePresetQuery),
    responses(
        (status = 204, description = "Preset deleted"),
        (status = 404, description = "No such preset", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_preset(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DeletePresetQuery>,
) -> Result<StatusCode, ErrorService> {
    state.services().await?.preset_store.delete(auth.id, query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
