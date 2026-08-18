use std::sync::Arc;

use crate::{
    cache::user_cache::UserCacheService,
    facade::{agent::Agent, prompt::PromptFacade},
    services::{chat_store::ChatStore, llm::OllamaService, settings_store::SettingsStore, tools::ToolService},
};

pub struct AppState {
    pub ollama: Arc<OllamaService>,
    pub chat_store: Arc<ChatStore>,
    pub tools: Arc<ToolService>,
    pub settings_store: Arc<SettingsStore>,
    pub agent: Agent,
    pub prompt: PromptFacade,
    pub user_cache: Arc<UserCacheService>,
}
