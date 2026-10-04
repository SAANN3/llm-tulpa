use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    facade::model_removal::{self, RemovedModel},
    routes::auth::OwnerUser,
    services::error::ErrorService,
    state::AppState,
};

#[derive(Deserialize, IntoParams)]
pub(crate) struct RemoveModelQuery {
    id: i64,
    /// Delete the `.gguf` file from the model folder too; without it only the database entry goes
    #[serde(default)]
    delete_file: bool,
}

/// Owner-only. Removes a model the backend runs itself, with its launch profiles. Chats on it move to
/// their owner's default model; sampling presets made for it stay as presets for any model. With
/// `delete_file` the file is deleted as well.
#[utoipa::path(
    delete,
    path = "/api/runtime/models",
    tag = "runtime",
    params(RemoveModelQuery),
    responses(
        (status = 200, description = "Removed", body = RemovedModel),
        (status = 403, description = "Only the owner can remove models", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such model", body = crate::services::error::ErrorBody),
        (status = 409, description = "It is the only model", body = crate::services::error::ErrorBody),
        (status = 423, description = "A turn is running on it", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn remove_model(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Query(query): Query<RemoveModelQuery>,
) -> Result<Json<RemovedModel>, ErrorService> {
    let services = state.services().await?;
    Ok(Json(model_removal::remove_model(&services, &state.runtime, &state.library, query.id, query.delete_file).await?))
}
