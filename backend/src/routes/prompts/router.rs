use std::sync::Arc;

use axum::{routing::post, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::chat_name::*;
use super::greet::*;
use super::input_examples::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/greet", post(greet))
        .route("/chat_name", post(chat_name))
        .route("/input_examples", post(input_examples))
}

#[derive(OpenApi)]
#[openapi(
    paths(greet, chat_name, input_examples),
    components(schemas(crate::facade::prompt::GreetOut, ChatNameRequest, InputExampleOut)),
)]
pub struct ApiDoc;
