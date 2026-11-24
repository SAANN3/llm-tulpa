use super::spec::{ParamSpec, check_params};
use crate::plugins::{base::PluginError, blueprint::NodeDefinition};
pub(super) type NodeSpec = NodeDefinition<ParamSpec>;

impl NodeDefinition<ParamSpec> {
    pub fn checked(self) -> Result<NodeDefinition, PluginError> {
        check_params(&self.node_type, &self.inputs).map_err(PluginError::InvalidState)?;
        check_params(&self.node_type, &self.outputs).map_err(PluginError::InvalidState)?;
        Ok(NodeDefinition {
            node_type: self.node_type,
            version: self.version,
            title: self.title,
            description: self.description,
            inputs: self.inputs.iter().map(ParamSpec::property_info).collect(),
            outputs: self.outputs.iter().map(ParamSpec::property_info).collect(),
            control_outputs: self.control_outputs,
            timeout_s: self.timeout_s,
        })
    }
}
