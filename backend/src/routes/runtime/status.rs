use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::AuthUser, services::llama_runtime::RuntimeStatus, state::AppState};

/// What the model server is doing: stopped, loading, ready or failed (with the reason), which launch
/// profile it runs, who has a turn on it right now, and where the model went (layers on the GPU,
/// buffer sizes) as the server itself reported at startup. A model change also arrives as a
/// `model_state` event on `/api/events`, so a page needn't poll while it waits.
#[utoipa::path(
    get,
    path = "/api/runtime",
    tag = "runtime",
    responses((status = 200, description = "The model server's state", body = RuntimeStatus)),
)]
pub async fn status(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Json<RuntimeStatus> {
    Json(state.runtime.status())
}
