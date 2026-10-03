use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, llama_runtime::RuntimeStatus},
    state::AppState,
};

/// Owner-only. Stops the model server, freeing the GPU; the next request that needs a model starts
/// it again. Refused with 423 while somebody has a turn running on it.
#[utoipa::path(
    post,
    path = "/api/runtime/stop",
    tag = "runtime",
    responses(
        (status = 200, description = "Stopped", body = RuntimeStatus),
        (status = 403, description = "Only the owner can stop the model server", body = crate::services::error::ErrorBody),
        (status = 409, description = "The server is an external one", body = crate::services::error::ErrorBody),
        (status = 423, description = "Another user has a turn running on the loaded model", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn stop(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Result<Json<RuntimeStatus>, ErrorService> {
    state.runtime.stop().await?;
    Ok(Json(state.runtime.status()))
}
