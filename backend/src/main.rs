mod cache;
mod config;
mod facade;
mod plugins;
mod routes;
mod services;
mod state;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{HeaderValue, Method};
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa_swagger_ui::SwaggerUi;

use services::{auth::AuthService, bootstrap::bootstrap, llm::OllamaService, tools::ToolService};
use state::AppState;
use tools::base::Tool;
use tools::temperature::TemperatureTool;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx::query=warn".into()),
        )
        .init();

    let config = config::load_or_init();
    let ollama_url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| config.ollama.url.clone());
    let ollama_context_length: u64 = std::env::var("OLLAMA_CONTEXT_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(config.ollama.context_length);

    let files_dir = match std::env::var("FILES_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => dirs::home_dir()
            .unwrap_or_else(|| panic!("could not determine home directory; set FILES_DIR to override"))
            .join(".llm-tulpa/files"),
    };

    let agent_history_len: u64 = std::env::var("AGENT_HISTORY_LEN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());

    let ollama = Arc::new(OllamaService::new(ollama_url, ollama_context_length as i32));

    let mut tool_list: Vec<Box<dyn Tool>> = vec![Box::new(TemperatureTool)];
    tool_list.extend(tools::os::collect());
    tool_list.extend(tools::storage::collect());
    tool_list.extend(tools::web::collect());
    tool_list.extend(tools::ui::collect());
    tool_list.extend(tools::files::collect());
    tool_list.extend(tools::llm::collect());
    let tools = Arc::new(ToolService::new(tool_list));

    let auth = AuthService::new(&config.jwt_secret);

    // The database connection comes only from the config file, never from the environment
    let db_connection = config.database.as_ref().map(|db| (db.base_url(), db.name.clone()));

    let services = if let Some((base_url, db_name)) = db_connection {
        match bootstrap(
            &base_url,
            &db_name,
            files_dir.clone(),
            ollama.clone(),
            tools.clone(),
            agent_history_len,
            ollama_context_length,
        )
        .await
        {
            Ok(svc) => Some(svc),
            Err(e) => {
                tracing::warn!("could not connect to the database at startup ({e}); starting in setup mode");
                None
            }
        }
    } else {
        tracing::info!("no database configured yet; starting in setup mode");
        None
    };

    let plugin_registry = services.as_ref().map(|s| s.plugin_registry.clone());

    let state = Arc::new(AppState {
        config: Arc::new(RwLock::new(config)),
        auth,
        ollama,
        tools,
        files_dir,
        agent_history_len,
        ollama_context_length,
        services: Arc::new(RwLock::new(services)),
    });

    let plugin_router = match plugin_registry {
        Some(registry) => routes::plugins::router::router(&registry).await,
        None => Router::new(),
    };

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            origin
                .to_str()
                .is_ok_and(|origin| origin.starts_with("http://") && origin.ends_with(":5173"))
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    let protected = routes::router::router().layer(from_fn_with_state(state.clone(), routes::auth::require_auth));
    let public = Router::new()
        .route("/auth/login", post(routes::auth::login))
        .route("/setup/status", get(routes::setup::status))
        .route("/setup/database", post(routes::setup::database))
        .route("/setup/owner", post(routes::setup::owner));

    let api = Router::new().merge(protected).merge(public);

    let app = Router::new()
        .nest("/api", api)
        .nest("/api/plugins", plugin_router)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", routes::router::openapi()))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
