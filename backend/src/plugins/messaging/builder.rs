use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::plugin::{MessagingPlugin, MessagingSettings};
use super::provider::MessagingProvider;
use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::plugins::env::PluginEnv;
use crate::tools::base::PropertyInfo;

/// The generic half of a messaging plugin's factory — implements `PluginBuilder` once
/// for every provider `P`, mirroring `MessagingPlugin`'s own reasoning. The `agent`/
/// `chat_store` of the `PluginEnv` are handed straight through to every `MessagingPlugin`
/// this builds — see `MessagingPlugin`'s own fields for what each is for.
pub struct MessagingProviderBuilder<P: MessagingProvider> {
    _provider: PhantomData<P>,
}

impl<P: MessagingProvider> MessagingProviderBuilder<P> {
    pub fn new() -> Self {
        Self { _provider: PhantomData }
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

    fn summary(&self) -> &str {
        P::summary()
    }

    fn help_message(&self) -> String {
        P::help_message()
    }

    async fn build(&self, settings: Value, env: &PluginEnv) -> Result<Arc<dyn Plugin>, PluginError> {
        let settings: MessagingSettings<P::Settings> = serde_json::from_value(settings)?;
        let provider = P::connect(settings.provider.clone()).await?;
        Ok(Arc::new(MessagingPlugin::new(provider, settings, env.agent.clone(), env.chat_store.clone())))
    }
}
