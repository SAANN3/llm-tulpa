use crate::{Host, Param};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize)]
pub struct NodeSpec {
    pub node_type: String,
    pub version: u32,
    pub title: String,
    pub description: String,
    pub inputs: Vec<Param>,
    pub outputs: Vec<Param>,
    pub control_outputs: Vec<String>,
    pub timeout_s: u64,
}

#[derive(Clone, Debug)]
pub struct NodeContext {
    pub user_id: i64,
    pub blueprint_id: i64,
    pub run_id: i64,
    pub node_id: String,
    pub host: Host,
    pub execution_id: u32,
    pub iteration: Vec<NodeIteration>,
}

/// An enclosing loop iteration or Merge delivery; Merge indices count deliveries within the run.
#[derive(Clone, Debug, Deserialize)]
pub struct NodeIteration {
    pub node_id: String,
    pub index: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct NodeResult {
    pub control_output: Option<String>,
    pub outputs: Map<String, Value>,
}
