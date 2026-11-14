use super::{
    definition::{Literal, NodeKind},
    execution::{MAX_VALUE_BYTES, Outcome, Values},
    runs::RunDetail,
    validation,
};
use crate::plugins::{
    blueprint::NodeContext, blueprint::NodeRegistration, registry::PluginRegistry,
};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) struct PluginNodes {
    plugins: Arc<PluginRegistry>,
}
impl PluginNodes {
    pub fn new(plugins: Arc<PluginRegistry>) -> Self {
        Self { plugins }
    }

    pub async fn catalog(&self) -> Vec<NodeRegistration> {
        self.plugins.blueprint_nodes().await
    }

    pub async fn execute(
        &self,
        user_id: i64,
        run: &RunDetail,
        node_id: &str,
        kind: &NodeKind,
        cancel: &CancellationToken,
    ) -> Result<Outcome, String> {
        let NodeKind::Plugin {
            node_type,
            version,
            arguments,
            ..
        } = kind
        else {
            return Err("expected a Plugin node".into());
        };
        let (plugin, registration) = self
            .plugins
            .blueprint_handler(kind)
            .await
            .ok_or("plugin node/version is no longer available")?;
        let spec = &registration.definition;
        let mut values = Values {
            state: &run.state,
            remaining: MAX_VALUE_BYTES,
        };
        let arguments: BTreeMap<String, Literal> = arguments
            .iter()
            .map(|(name, input)| values.resolve(input, 0).map(|v| (name.clone(), v)))
            .collect::<Result<_, _>>()?;
        validation::plugin_values(&arguments, &spec.inputs)?;
        if cancel.is_cancelled() {
            return Err("run was stopped".into());
        }
        // Like Tool nodes, Stop waits for the current bounded call; it cannot roll back a plugin's side effects.
        let result = tokio::time::timeout(
            Duration::from_secs(spec.timeout_s),
            AssertUnwindSafe(plugin.execute_blueprint_node(
                node_type,
                *version,
                arguments,
                NodeContext {
                    user_id,
                    blueprint_id: run.run.blueprint_id,
                    run_id: run.run.id,
                    node_id: node_id.into(),
                },
            ))
            .catch_unwind(),
        )
        .await
        .map_err(|_| "plugin node timed out")?
        .map_err(|_| "plugin node panicked")?
        .map_err(|e| e.to_string())?;
        if serde_json::to_vec(&result)
            .map_err(|_| "could not serialize plugin node result")?
            .len()
            > MAX_VALUE_BYTES
        {
            return Err("plugin node output exceeds 1 MiB".into());
        }
        validation::plugin_values(&result.outputs, &spec.outputs)?;
        match &result.control_output {
            Some(port) if !spec.control_outputs.contains(port) => {
                return Err("plugin selected an undeclared control output".into());
            }
            None if !spec.control_outputs.is_empty() => {
                return Err("plugin must select one declared control output".into());
            }
            _ => {}
        }
        Ok(Outcome {
            assignment: None,
            control_output: result.control_output,
            outputs: result.outputs,
        })
    }
}
