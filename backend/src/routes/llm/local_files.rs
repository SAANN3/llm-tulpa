use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::OwnerUser, services::model_library::LocalFiles, state::AppState};

/// Owner-only. The `.gguf` files sitting in the configured model directory (`model_dir` in
/// settings.json — the same folder Ollama's `MODEL_DIR` points at), ready to be imported as
/// models with `POST /api/llm/import`. Each file is classified from its own GGUF header (model, vision projector, or invalid), and every model lists the projectors that fit it and, when there's a clear answer, the one to preselect.
#[utoipa::path(
    get,
    path = "/api/llm/local_files",
    tag = "llm",
    responses(
        (status = 200, description = "Model files on disk; `configured: false` when no directory is set", body = LocalFiles),
        (status = 403, description = "Only the owner can list local files", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn local_files(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<LocalFiles> {
    Json(state.library.local_files().await)
}
