use std::sync::Arc;

use axum::{routing::{get, post}, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::chat_name::*;
use super::folder_name::*;
use super::blueprint_name::*;
use super::greet::*;
use super::home_lines::*;
use super::input_examples::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/greet", post(greet))
        .route("/chat_name", post(chat_name))
        .route("/folder_name", post(folder_name))
        .route("/blueprint_name", post(blueprint_name))
        .route("/input_examples", post(input_examples))
        .route("/home_lines", get(home_lines))
}

#[derive(OpenApi)]
#[openapi(
    paths(greet, chat_name, folder_name, blueprint_name, input_examples, home_lines),
    components(schemas(crate::facade::prompt::GreetOut, ChatNameRequest, FolderNameRequest, BlueprintNameRequest, InputExampleOut, HomeLinesOut)),
)]
pub struct ApiDoc;
