use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::catalog::*;
use super::generate::*;
use super::models::*;
use super::pull::*;
use super::thinking_capability::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/generate", post(generate))
        .route("/thinking_capability", get(thinking_capability))
        .route("/models", get(models))
        .route("/catalog", get(catalog))
        .route("/pull", post(pull))
}

#[derive(OpenApi)]
#[openapi(
    paths(generate, thinking_capability, models, catalog, pull),
    components(schemas(
        GenerateRequest,
        GenerateResponse,
        CatalogResponse,
        PullRequest,
        crate::services::llm::ThinkingCapability,
        crate::services::llm::LocalModel,
        crate::services::llm::LocalModelDetails,
    ))
)]
pub struct ApiDoc;
