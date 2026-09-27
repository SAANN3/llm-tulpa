use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::plugins::coding::signatures::plugin::SignaturesPlugin;
use crate::tools::base::PropertyInfo;

/// Builds the single `signatures` instance. No settings to validate — an empty
/// schema is what makes `PluginRegistry::register` give it a built, enableable
/// instance right away instead of leaving it stuck waiting for a settings form.
pub struct SignaturesBuilder;

#[async_trait]
impl PluginBuilder for SignaturesBuilder {
    fn plugin_name(&self) -> &str {
        "coding"
    }

    fn plugin_subname(&self) -> &str {
        "signatures"
    }

    fn settings_schema(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    fn help_message(&self) -> String {
        "Gives the model signature-level views of code files and dependencies, instead of full file contents or LSP features. \
         additional setup, depending on language, may be needed"
            .to_string()
    }

    async fn build(&self, _settings: Value) -> Result<Arc<dyn Plugin>, PluginError> {
        Ok(Arc::new(SignaturesPlugin))
    }
}
