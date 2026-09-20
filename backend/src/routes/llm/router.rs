use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::catalog::*;
use super::generate::*;
use super::import::*;
use super::local_files::*;
use super::models::*;
use super::pull::*;
use super::tasks::*;
use super::thinking_capability::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/generate", post(generate))
        .route("/thinking_capability", get(thinking_capability))
        .route("/models", get(models))
        .route("/catalog", get(catalog))
        .route("/local_files", get(local_files))
        .route("/pull", post(pull))
        .route("/import", post(import))
        .route("/tasks", get(tasks))
}

#[derive(OpenApi)]
#[openapi(
    paths(generate, thinking_capability, models, catalog, local_files, pull, import, tasks),
    components(schemas(
        GenerateRequest,
        GenerateResponse,
        PullRequest,
        ImportBody,
        ImportFile,
        crate::services::llm::ThinkingCapability,
        crate::services::llm::LocalModel,
        crate::services::llm::LocalModelDetails,
        crate::services::model_library::Catalog,
        crate::services::model_library::CatalogModel,
        crate::services::model_library::LocalFiles,
        crate::services::model_library::LocalFile,
        crate::services::model_library::LocalFileKind,
        crate::services::model_library::ModelTask,
        crate::services::model_library::TaskKind,
        crate::services::model_library::TaskState,
    ))
)]
pub struct ApiDoc;
