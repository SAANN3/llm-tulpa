mod cache;
mod facade;
mod plugins;
mod routes;
mod services;
mod state;
mod tools;

use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa_swagger_ui::SwaggerUi;

use cache::user_cache::UserCacheService;
use facade::{agent::Agent, prompt::PromptFacade};
use plugins::base::PluginBuilder;
use plugins::messaging::builder::MessagingProviderBuilder;
use plugins::messaging::discord::DiscordProvider;
use plugins::messaging::telegram::TelegramProvider;
use plugins::messaging::vk::VkProvider;
use plugins::registry::PluginRegistry;
use services::{
    chat_store::ChatStore, llm::OllamaService, permission_store::PermissionStore,
    plugin_settings_store::PluginSettingsStore, settings_store::SettingsStore, tools::ToolService,
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

    let database_name = std::env::var("DATABASE_NAME")
        .unwrap_or_else(|_| "llm_tulpa".to_string());

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
    
    // Build tool list with all domains
    let mut tool_list: Vec<Box<dyn Tool>> = vec![Box::new(TemperatureTool)];
    tool_list.extend(tools::os::collect());       // os.* tools (hardware, disk, processes, network, env vars, etc.)
    tool_list.extend(tools::storage::collect());  // storage.* tools (read/write/modify files)
    tool_list.extend(tools::web::collect());      // web.* tools (download files)
    
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
    let plugin_settings_store = Arc::new(PluginSettingsStore::new(&database_url, &database_name).await);
    let plugin_registry = Arc::new(PluginRegistry::new(plugin_settings_store));

    // A plugin chat's own `Agent` — empty `ToolService`, so a plugin conversation gets
    // real persistence and a real reply with zero tool-calling risk, while still
    // sharing `ollama`/`chat_store`/`permission_store` with the main app's `Agent`
    // above rather than duplicating those services.
    let plugin_agent = Arc::new(Agent::new(
        ollama.clone(),
        chat_store.clone(),
        Arc::new(ToolService::new(vec![])),
        permission_store.clone(),
        agent_history_len,
        ollama_context_length,
    ));

    let plugin_builders: Vec<Arc<dyn PluginBuilder>> = vec![
        Arc::new(MessagingProviderBuilder::<TelegramProvider>::new(plugin_agent.clone(), chat_store.clone())),
        Arc::new(MessagingProviderBuilder::<DiscordProvider>::new(plugin_agent.clone(), chat_store.clone())),
        Arc::new(MessagingProviderBuilder::<VkProvider>::new(plugin_agent, chat_store.clone())),
    ];
    plugin_registry
        .register_many(plugin_builders)
        .await
        .expect("registering the known plugin builders should never fail");

    let state = Arc::new(AppState {
        ollama,
        chat_store,
        tools,
        settings_store,
        agent,
        prompt,
        user_cache,
        plugin_registry,
    });

    // Built separately from `routes::router::router()` — see that function's own doc
    // comment for why the plugins domain doesn't nest inside it like the others.
    let plugin_router = routes::plugins::router::router(&state).await;

    // Matches on port alone (`:5173`, the frontend's fixed published port) rather
    // than a fixed host — the frontend can be reached at `localhost`, a LAN IP, or
    // anything else depending on which device's browser is asking, and a fixed
    // origin here would only ever match one of those.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            origin
                .to_str()
                .is_ok_and(|origin| origin.starts_with("http://") && origin.ends_with(":5173"))
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .nest("/api", routes::router::router())
        .nest("/api/plugins", plugin_router)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", routes::router::openapi()))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
