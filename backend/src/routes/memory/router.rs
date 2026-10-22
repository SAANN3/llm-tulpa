use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::clear::*;
use super::create::*;
use super::delete::*;
use super::dto::*;
use super::get::*;
use super::update::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_memory).post(create_memory).delete(delete_memory))
        .route("/update", post(update_memory))
        .route("/clear", post(clear_memory))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_memory, create_memory, update_memory, delete_memory, clear_memory),
    components(schemas(MemoryEntryOut, MemoryOut, MemoryEntryIn, UpdateMemoryRequest, ClearedOut)),
)]
pub struct ApiDoc;
