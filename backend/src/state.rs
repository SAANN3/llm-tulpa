use std::path::PathBuf;
use std::sync::Arc;
use axum::http::StatusCode;
use tokio::sync::RwLock;
use crate::config::AppConfig;
use crate::services::{
    auth::AuthService, bootstrap::AppServices, error::ErrorService, llm::OllamaService,
    tools::ToolService,
};

pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub auth: AuthService,
    pub ollama: Arc<OllamaService>,
    pub tools: Arc<ToolService>,
    pub files_dir: PathBuf,
    pub agent_history_len: u64,
    pub ollama_context_length: u64,
    pub services: Arc<RwLock<Option<AppServices>>>,
}

impl AppState {
    pub async fn services(&self) -> Result<AppServices, ErrorService> {
        self.services.read().await.clone().ok_or_else(|| {
            ErrorService::new(StatusCode::SERVICE_UNAVAILABLE, "the backend is not configured yet")
        })
    }
}
