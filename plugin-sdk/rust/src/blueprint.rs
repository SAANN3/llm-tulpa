use crate::{Host, Param};
use serde::Serialize;
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
}
#[derive(Clone, Debug, Serialize)]
pub struct NodeResult {
    pub port: Option<String>,
    pub outputs: Map<String, Value>,
}
