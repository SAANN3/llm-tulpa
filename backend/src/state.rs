use std::sync::Arc;

use crate::{
    cache::user_cache::UserCacheService,
    facade::{agent::Agent, prompt::PromptFacade},
    plugins::registry::PluginRegistry,
    services::{
        chat_store::ChatStore, file_store::FileStore, llm::OllamaService, settings_store::SettingsStore,
        tools::ToolService,
    },
};

pub struct AppState {
    pub ollama: Arc<OllamaService>,
    pub chat_store: Arc<ChatStore>,
    pub tools: Arc<ToolService>,
    pub settings_store: Arc<SettingsStore>,
    pub file_store: Arc<FileStore>,
    pub agent: Agent,
    pub prompt: PromptFacade,
    pub user_cache: Arc<UserCacheService>,
    pub plugin_registry: Arc<PluginRegistry>,
}
