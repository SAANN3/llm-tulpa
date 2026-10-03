mod cache;
mod config;
mod facade;
mod plugins;
mod routes;
mod services;
mod state;
mod tools;

use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::middleware::from_fn_with_state;
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa_swagger_ui::SwaggerUi;

use services::{event_bus::EventBus, model_folder::ModelFolder, llama_runtime::LlamaRuntime, llm::{LlamaCppProvider, LlmProviders, OllamaService}, tools::ToolService};
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
    tool_list.extend(tools::chat::collect());
    let tools = Arc::new(ToolService::new(tool_list));

    let model_folder = ModelFolder::new(config.model_dir.clone());
    let events = Arc::new(EventBus::new());
    let runtime = LlamaRuntime::new(config.llama_cpp.clone(), events.clone());
    ollama.share_gpu_with(runtime.clone());
    runtime.set_before_start({
        let ollama = ollama.clone();
        move || {
            let ollama = ollama.clone();
            Box::pin(async move { ollama.unload_all().await })
        }
    });
    let llama_cpp = Arc::new(LlamaCppProvider::new(runtime.clone(), model_folder.clone(), config.llama_cpp.base_url(), config.ollama.context_length));
    let providers = LlmProviders::new(llama_cpp, vec![ollama.clone()]);

    let state = Arc::new(AppState::new(config, ollama, providers, tools, events, runtime.clone(), model_folder));
    let shutdown = state.shutdown.clone();

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
        // Named, not `*`: a wildcard doesn't cover `Authorization`, which browsers are about to enforce
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]);

    let protected = routes::router::router().layer(from_fn_with_state(state.clone(), routes::auth::require_auth));
    let api = Router::new().merge(protected).merge(routes::router::public_router());

    let app = Router::new()
        .nest("/api", api)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", routes::router::openapi()))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal(shutdown)).await.unwrap();

    // The model server is the backend's child: leaving it behind would keep the model in VRAM.
    runtime.shutdown().await;
}

/// Resolves on Ctrl-C or, on Unix, SIGTERM (what `docker stop` and a service manager send).
async fn shutdown_signal(shutdown: tokio_util::sync::CancellationToken) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    // Open event streams would otherwise keep the server waiting for ever
    shutdown.cancel();
}
