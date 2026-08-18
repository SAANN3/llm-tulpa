use std::sync::Arc;

use axum::{http::StatusCode, Router};
use utoipa::OpenApi;

use crate::{services::error::ErrorService, state::AppState};

use super::{agent, chats, llm, prompts, settings};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/llm", llm::router::router())
        .nest("/agent", agent::router::router())
        .nest("/chats", chats::router::router())
        .nest("/prompts", prompts::router::router())
        .nest("/settings", settings::router::router())
        .fallback(not_found)
}

async fn not_found() -> ErrorService {
    ErrorService::new(StatusCode::NOT_FOUND, "no route matches this path")
}

/// The whole app's OpenAPI document — each route domain builds its own `ApiDoc` from
/// handlers only it can see (route handler modules are private to their domain; only
/// `router` is public), so this just merges the three together.
pub fn openapi() -> utoipa::openapi::OpenApi {
    llm::router::ApiDoc::openapi()
        .merge_from(agent::router::ApiDoc::openapi())
        .merge_from(chats::router::ApiDoc::openapi())
        .merge_from(prompts::router::ApiDoc::openapi())
        .merge_from(settings::router::ApiDoc::openapi())
}
