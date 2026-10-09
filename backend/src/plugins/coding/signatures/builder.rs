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

    fn summary(&self) -> &str {
        "A dependency's API for the model: its types and signatures, not its whole source"
    }

    fn help_message(&self) -> String {
        "Gives the model a trimmed, typed view of a dependency's public API (its types, and its functions and methods \
         with their real signatures) instead of its whole source. Nothing to set up: a dependency is found the way the \
         project itself finds it (cargo, node_modules, go, dotnet, Maven, pkg-config), so that toolchain has to be \
         available where the model's commands run, and the project's dependencies installed."
            .to_string()
    }

    async fn build(&self, _settings: Value) -> Result<Arc<dyn Plugin>, PluginError> {
        Ok(Arc::new(SignaturesPlugin))
    }
}
