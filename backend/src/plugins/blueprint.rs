use crate::{
    facade::blueprint::{definition::{Literal, NodeKind}, runs::Iteration},
    tools::base::PropertyInfo,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use utoipa::ToSchema;

#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeDefinition<P = PropertyInfo> {
    pub node_type: String,
    pub version: u32,
    pub title: String,
    pub description: String,
    pub inputs: Vec<P>,
    pub outputs: Vec<P>,
    pub control_outputs: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_s: u64,
}

fn default_timeout() -> u64 {
    30
}

#[derive(Clone, Serialize, ToSchema)]
pub struct NodeRegistration {
    pub plugin_name: String,
    pub plugin_subname: String,
    #[serde(flatten)]
    pub definition: NodeDefinition,
}
impl NodeRegistration {
    pub fn matches(&self, node: &NodeKind) -> bool {
        matches!(node, NodeKind::Plugin { plugin_name, plugin_subname, node_type, version, .. }
            if plugin_name == &self.plugin_name && plugin_subname == &self.plugin_subname
                && node_type == &self.definition.node_type && *version == self.definition.version)
    }
}

#[derive(Serialize)]
pub struct NodeContext {
    pub user_id: i64,
    pub blueprint_id: i64,
    pub run_id: i64,
    pub node_id: String,
    pub execution_id: u32,
    pub iteration: Vec<Iteration>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeResult {
    pub control_output: Option<String>,
    pub outputs: BTreeMap<String, Literal>,
}

pub fn check_definitions(nodes: &[NodeDefinition]) -> Result<(), String> {
    fn identifier(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 128
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    }
    if nodes.len() > 128 {
        return Err("a plugin can declare at most 128 node definitions".into());
    }
    let mut keys = HashSet::new();
    for node in nodes {
        if !identifier(&node.node_type)
            || node.version == 0
            || !keys.insert((&node.node_type, node.version))
        {
            return Err(
                "node identities must be valid, versioned and unique within the plugin".into(),
            );
        }
        if node.title.trim().is_empty()
            || node.title.len() > 160
            || node.description.len() > 8192
            || !(1..=3600).contains(&node.timeout_s)
        {
            return Err("invalid node title, description or timeout (1..3600 seconds)".into());
        }
        for names in [
            node.inputs.iter().map(|p| &p.name).collect::<Vec<_>>(),
            node.outputs.iter().map(|p| &p.name).collect(),
            node.control_outputs.iter().collect(),
        ] {
            let mut seen = HashSet::new();
            if names.len() > 128
                || names
                    .into_iter()
                    .any(|name| !identifier(name) || !seen.insert(name))
            {
                return Err("node port names must be valid and unique within their group".into());
            }
        }
    }
    Ok(())
}
