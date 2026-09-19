use std::sync::Arc;

use axum::{routing::get, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::stream::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(stream))
}

#[derive(OpenApi)]
#[openapi(paths(stream), components(schemas(crate::services::event_bus::ServerEvent)))]
pub struct ApiDoc;
