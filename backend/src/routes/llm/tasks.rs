use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::OwnerUser, services::model_library::ModelTask, state::AppState};

/// Owner-only. The model pulls and imports running now, plus those that finished in the last
/// hour — what a client polls to show progress and to learn when a model has become available.
#[utoipa::path(
    get,
    path = "/api/llm/tasks",
    tag = "llm",
    responses(
        (status = 200, description = "Running and recently finished tasks", body = [ModelTask]),
        (status = 403, description = "Only the owner can see model tasks", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn tasks(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<Vec<ModelTask>> {
    Json(state.library.tasks())
}
