use std::sync::Arc;

use axum::{http::StatusCode, Router};
use utoipa::OpenApi;

use crate::{services::error::ErrorService, state::AppState};

use super::{agent, auth, chats, files, llm, plugins, prompts, settings, setup, users};

/// Every route that needs a signed-in user — `main.rs` puts `require_auth` in front of the
/// whole thing. Owner-only routes (users, plugins) are gated per handler by the `OwnerUser`
/// extractor, so they sit alongside the rest.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/auth", auth::router::router())
        .nest("/llm", llm::router::router())
        .nest("/agent", agent::router::router())
        .nest("/chats", chats::router::router())
        .nest("/files", files::router::router())
        .nest("/plugins", plugins::router::router())
        .nest("/prompts", prompts::router::router())
        .nest("/settings", settings::router::router())
        .nest("/users", users::router::router())
        .fallback(not_found)
}

/// The few routes that can't require a token: signing in, and first-run setup (which exists
/// for the state where there's no database, so no accounts to sign in with).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/auth", auth::router::public_router())
        .nest("/setup", setup::router::public_router())
}

async fn not_found() -> ErrorService {
    ErrorService::new(StatusCode::NOT_FOUND, "no route matches this path")
}

/// The whole app's OpenAPI document — each route domain builds its own `ApiDoc` from
/// handlers only it can see (route handler modules are private to their domain; only
/// `router` is public), so this just merges them together.
pub fn openapi() -> utoipa::openapi::OpenApi {
    llm::router::ApiDoc::openapi()
        .merge_from(agent::router::ApiDoc::openapi())
        .merge_from(auth::router::ApiDoc::openapi())
        .merge_from(chats::router::ApiDoc::openapi())
        .merge_from(files::router::ApiDoc::openapi())
        .merge_from(plugins::router::ApiDoc::openapi())
        .merge_from(prompts::router::ApiDoc::openapi())
        .merge_from(settings::router::ApiDoc::openapi())
        .merge_from(setup::router::ApiDoc::openapi())
        .merge_from(users::router::ApiDoc::openapi())
}
