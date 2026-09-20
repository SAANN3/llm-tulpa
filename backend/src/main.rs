mod cache;
mod config;
mod facade;
mod plugins;
mod routes;
mod services;
mod state;
mod tools;

use std::sync::Arc;

use axum::http::{HeaderValue, Method};
use axum::middleware::from_fn_with_state;
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa_swagger_ui::SwaggerUi;

use services::{llm::OllamaService, tools::ToolService};
use state::AppState;
use tools::base::Tool;
use tools::temperature::TemperatureTool;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx::query=warn".into()),
        )
        .init();

    // Everything the backend is configured with comes from `settings.json` (see `config.rs`);
    // the environment only carries `RUST_LOG`.
    let config = config::load_or_init();
    config::install_tool_settings(&config);
    let bind_addr = config.bind_addr.clone();

    let ollama = Arc::new(OllamaService::new(config.ollama.url.clone(), config.ollama.context_length as i32));

    let mut tool_list: Vec<Box<dyn Tool>> = vec![Box::new(TemperatureTool)];
    tool_list.extend(tools::os::collect());
    tool_list.extend(tools::storage::collect());
    tool_list.extend(tools::web::collect());
    tool_list.extend(tools::ui::collect());
    tool_list.extend(tools::files::collect());
    tool_list.extend(tools::llm::collect());
    let tools = Arc::new(ToolService::new(tool_list));

    let state = Arc::new(AppState::new(config, ollama, tools));

    // No database configured (first run) or an unreachable one: start in setup mode, and let
    // the wizard (or a later status poll) bring the services up.
    match state.connect_configured().await {
        Ok(true) => {}
        Ok(false) => tracing::info!("no database configured yet; starting in setup mode"),
        Err(e) => tracing::warn!("could not connect to the database at startup ({e}); starting in setup mode"),
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            origin
                .to_str()
                .is_ok_and(|origin| origin.starts_with("http://") && origin.ends_with(":5173"))
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let protected = routes::router::router().layer(from_fn_with_state(state.clone(), routes::auth::require_auth));
    let api = Router::new().merge(protected).merge(routes::router::public_router());

    let app = Router::new()
        .nest("/api", api)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", routes::router::openapi()))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
