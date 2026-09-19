use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{
        error::ErrorService,
        model_library::{ImportRequest, ModelTask},
    },
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ImportFile {
    /// The model file, as listed by `GET /api/llm/local_files` (relative to the model directory).
    file: String,
    /// What to call the model in Ollama; derived from the file name when omitted.
    name: Option<String>,
    /// A vision projector (`mmproj`) file to pair with it, likewise relative.
    projector: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct ImportBody {
    /// One entry per model to create — several at once is the point.
    imports: Vec<ImportFile>,
}

/// Owner-only. Imports local `.gguf` files as Ollama models, one background task each (watch
/// them at `GET /api/llm/tasks`), without downloading anything. Every entry is checked before
/// any starts — a wrong path or a name that's already taken fails the whole call.
#[utoipa::path(
    post,
    path = "/api/llm/import",
    tag = "llm",
    request_body = ImportBody,
    responses(
        (status = 202, description = "Imports started", body = [ModelTask]),
        (status = 400, description = "A bad path or model name", body = crate::services::error::ErrorBody),
        (status = 403, description = "Only the owner can import models", body = crate::services::error::ErrorBody),
        (status = 409, description = "No model directory configured, or a model with that name already exists", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn import(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<ImportBody>,
) -> Result<(StatusCode, Json<Vec<ModelTask>>), ErrorService> {
    if body.imports.is_empty() {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "nothing to import"));
    }

    let requests = body
        .imports
        .into_iter()
        .map(|f| ImportRequest { file: f.file, name: f.name, projector: f.projector })
        .collect();

    Ok((StatusCode::ACCEPTED, Json(state.library.start_imports(requests).await?)))
}
