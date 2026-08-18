use std::sync::Arc;

use axum::{routing::post, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::generate::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/generate", post(generate))
}

#[derive(OpenApi)]
#[openapi(paths(generate), components(schemas(GenerateRequest, GenerateResponse)))]
pub struct ApiDoc;
