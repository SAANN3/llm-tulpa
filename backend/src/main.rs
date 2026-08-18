mod cache;
mod facade;
mod routes;
mod services;
mod state;
mod tools;

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Method};
use tower_http::cors::CorsLayer;
use utoipa_swagger_ui::SwaggerUi;

use cache::user_cache::UserCacheService;
use facade::{agent::Agent, prompt::PromptFacade};
use services::{
    chat_store::ChatStore, llm::OllamaService, permission_store::PermissionStore,
    settings_store::SettingsStore, tools::ToolService,
};
use state::AppState;
use tools::base::Tool;
use tools::temperature::TemperatureTool;

#[tokio::main]
async fn main() {
    // sqlx logs every query at INFO by default, which drowns out our own logs — quiet
    // it down to WARN unless RUST_LOG says otherwise.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx::query=warn".into()),
        )
        .init();

    let ollama_url =
        std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

    let ollama_model_name =
        std::env::var("OLLAMA_MODEL_NAME").unwrap_or_else(|_| "local-llm".to_string());

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432".to_string());

    let database_name = std::env::var("DATABASE_NAME").unwrap_or_else(|_| "llm_tulpa".to_string());

    let agent_history_len: u64 = std::env::var("AGENT_HISTORY_LEN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);

    // Single source of truth for the model's context window — same name and default
    // as `llm/compose.yaml`'s `OLLAMA_CONTEXT_LENGTH`, so setting it once there keeps
    // Ollama's real context and this backend's own token-budget math (num_predict cap,
    // compaction thresholds) in sync instead of drifting apart.
    let ollama_context_length: u64 = std::env::var("OLLAMA_CONTEXT_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(32768);

    // Loopback-only by default so nothing on the LAN can reach this — see the
    // `main.rs` history for why this was deliberately narrowed from `0.0.0.0`.
    // Docker needs `0.0.0.0` inside the container for its own port publishing to
    // reach the process at all (a container's loopback isn't the host's), so the
    // compose setup sets this explicitly; native runs keep the old default.
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());

    let ollama = Arc::new(OllamaService::new(ollama_url, ollama_model_name, ollama_context_length as i32));
    let chat_store = Arc::new(ChatStore::new(&database_url, &database_name).await);
    let mut tool_list: Vec<Box<dyn Tool>> = vec![Box::new(TemperatureTool)];
    tool_list.extend(tools::os::collect());
    tool_list.extend(tools::storage::collect());
    let tools = Arc::new(ToolService::new(tool_list));
    let settings_store = Arc::new(SettingsStore::new(&database_url, &database_name).await);
    // Must come after `chat_store` — `tool_permissions` has a foreign key on `chats`,
    // so `chats` needs to already exist by the time this runs its own migration.
    let permission_store = Arc::new(PermissionStore::new(&database_url, &database_name).await);

    let agent = Agent::new(
        ollama.clone(),
        chat_store.clone(),
        tools.clone(),
        permission_store.clone(),
        agent_history_len,
        ollama_context_length,
    );
    let prompt = PromptFacade::new(ollama.clone());
    let user_cache = UserCacheService::new(settings_store.clone(), prompt.clone()).await;

    let state = Arc::new(AppState {
        ollama,
        chat_store,
        tools,
        settings_store,
        agent,
        prompt,
        user_cache,
    });

    let cors = CorsLayer::new()
        .allow_origin("http://localhost:5173".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .nest("/api", routes::router::router())
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", routes::router::openapi()))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();

}
