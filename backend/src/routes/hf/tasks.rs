use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::OwnerUser, services::hf_library::HfTask, state::AppState};

/// Owner-only. The running and recently finished Hugging Face downloads.
#[utoipa::path(
    get,
    path = "/api/hf/tasks",
    tag = "hf",
    responses((status = 200, description = "Downloads", body = [HfTask])),
)]
pub async fn tasks(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<Vec<HfTask>> {
    Json(state.hf.tasks())
}
