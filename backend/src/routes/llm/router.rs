use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::generate::*;
use super::thinking_capability::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/generate", post(generate))
        .route("/thinking_capability", get(thinking_capability))
}

#[derive(OpenApi)]
#[openapi(
    paths(generate, thinking_capability),
    components(schemas(GenerateRequest, GenerateResponse, crate::services::llm::ThinkingCapability))
)]
pub struct ApiDoc;
