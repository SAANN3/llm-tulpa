use std::sync::Arc;

use axum::{http::StatusCode, Router};
use utoipa::OpenApi;

use crate::{services::error::ErrorService, state::AppState};

use super::{agent, chats, llm, plugins, prompts, settings};

/// The `/plugins` domain isn't nested here — unlike everything below, its route *set*
/// depends on runtime data (which plugins are registered), not just compile-time
/// structure, so building it needs an `.await` on the registry. Rather than infect this
/// otherwise-uniform, synchronous composition (and every domain's `router()` signature
/// it calls) with that one exception, `main.rs` mounts `routes::plugins::router::router`
/// as its own separate step, the same way it already handles other one-off async setup.
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
/// `router` is public), so this just merges the three together. `plugins` is included
/// here even though its `router()` is mounted separately (see that function's own doc
/// comment) — this function only merges static schema definitions, which has no
/// dependency on how/when the router itself gets built.
pub fn openapi() -> utoipa::openapi::OpenApi {
    llm::router::ApiDoc::openapi()
        .merge_from(agent::router::ApiDoc::openapi())
        .merge_from(chats::router::ApiDoc::openapi())
        .merge_from(prompts::router::ApiDoc::openapi())
        .merge_from(settings::router::ApiDoc::openapi())
        .merge_from(plugins::router::ApiDoc::openapi())
}
