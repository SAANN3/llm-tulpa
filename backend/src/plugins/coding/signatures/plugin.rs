use async_trait::async_trait;
use axum::Router;
use serde_json::{Value, json};

use crate::plugins::base::{Plugin, PluginError};
use crate::plugins::coding::signatures::get_signatures::GetSignaturesTool;
use crate::tools::base::Tool;

/// Gives the model signature-level insight into code — this codebase's own files and
/// its dependencies — without a full LSP. No settings, no background work, no API
/// routes: everything it offers is contributed through `tools()`, filled in next.
pub struct SignaturesPlugin;

#[async_trait]
impl Plugin for SignaturesPlugin {
    fn plugin_name(&self) -> &str {
        "coding"
    }

    fn plugin_subname(&self) -> &str {
        "signatures"
    }

    fn settings_value(&self) -> Value {
        json!({})
    }

    fn api_router(&self) -> Router {
        Router::new()
    }

    async fn on_enabled(&self) -> Result<(), PluginError> {
        Ok(())
    }

    async fn on_disabled(&self) -> Result<(), PluginError> {
        Ok(())
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(GetSignaturesTool)]
    }
}
