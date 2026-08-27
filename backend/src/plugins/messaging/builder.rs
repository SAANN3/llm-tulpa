use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::plugin::{MessagingPlugin, MessagingSettings};
use super::provider::MessagingProvider;
use crate::facade::agent::Agent;
use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::services::chat_store::ChatStore;
use crate::tools::base::PropertyInfo;

/// The generic half of a messaging plugin's factory — implements `PluginBuilder` once
/// for every provider `P`, mirroring `MessagingPlugin`'s own reasoning. `agent`/
/// `chat_store` are handed straight through to every `MessagingPlugin` this builds —
/// see `MessagingPlugin`'s own fields for what each is for.
pub struct MessagingProviderBuilder<P: MessagingProvider> {
    agent: Arc<Agent>,
    chat_store: Arc<ChatStore>,
    _provider: PhantomData<P>,
}

impl<P: MessagingProvider> MessagingProviderBuilder<P> {
    pub fn new(agent: Arc<Agent>, chat_store: Arc<ChatStore>) -> Self {
        Self { agent, chat_store, _provider: PhantomData }
    }
}

#[async_trait]
impl<P: MessagingProvider> PluginBuilder for MessagingProviderBuilder<P> {
    fn plugin_name(&self) -> &str {
        "messaging"
    }

    fn plugin_subname(&self) -> &str {
        P::subname()
    }

    fn settings_schema(&self) -> Vec<PropertyInfo> {
        super::plugin::settings_schema::<P>()
    }

    fn help_message(&self) -> String {
        P::help_message()
    }

    async fn build(&self, settings: Value) -> Result<Arc<dyn Plugin>, PluginError> {
        let settings: MessagingSettings<P::Settings> = serde_json::from_value(settings)?;
        let provider = P::connect(settings.provider.clone()).await?;
        Ok(Arc::new(MessagingPlugin::new(provider, settings, self.agent.clone(), self.chat_store.clone())))
    }
}
