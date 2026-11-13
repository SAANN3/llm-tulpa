use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::{llm::ThinkChoice, tool_selection_store::ToolMode};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub schema_version: u32,
    pub nodes: Vec<Node>,
    /// Array order is the execution order of edges leaving the same port.
    pub edges: Vec<Edge>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub position: Position,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub port: String,
    pub to: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Entry,
    Generate {
        model: ModelChoice,
        prompt: Input,
        #[serde(default)]
        think: Option<ThinkChoice>,
    },
    Agent {
        model: ModelChoice,
        prompt: Input,
        #[serde(default)]
        system_prompt: Option<Input>,
        tools: ToolChoice,
        #[serde(default)]
        think: Option<ThinkChoice>,
        #[serde(default)]
        reuse_chat: bool,
        #[serde(default)]
        write_notes: bool,
        #[serde(default)]
        write_memory: bool,
    },
    Tool {
        name: String,
        arguments: BTreeMap<String, Input>,
        #[serde(default)]
        require_confirmation: bool,
    },
    Sleep {
        seconds: Input,
    },
    If {
        left: Input,
        operator: Comparison,
        right: Input,
    },
    Switch {
        value: Input,
        cases: Vec<SwitchCase>,
    },
    Format {
        template: String,
        arguments: BTreeMap<String, Input>,
    },
    Return {
        values: BTreeMap<String, Input>,
    },
    /// An opaque node identity, not an installed plugin or permission to execute it.
    Plugin {
        plugin_name: String,
        plugin_subname: String,
        node_type: String,
        version: u32,
        arguments: BTreeMap<String, Input>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelChoice {
    UserDefault,
    Model {
        id: i64,
        #[serde(default)]
        launch_profile_id: Option<i64>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolChoice {
    pub mode: ToolMode,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    Contains,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SwitchCase {
    pub port: String,
    pub value: Literal,
}

/// Bindings are independent of control edges; a nested argument can reference another node too.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Literal {
        value: Literal,
    },
    Output {
        node_id: String,
        port: String,
        #[serde(default)]
        path: Vec<PathPart>,
    },
    Array {
        #[schema(no_recursion)]
        items: Vec<Input>,
    },
    Object {
        #[schema(no_recursion)]
        fields: BTreeMap<String, Input>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PathPart {
    Field(String),
    Index(u32),
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum Literal {
    Null,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    #[schema(no_recursion)]
    Array(Vec<Literal>),
    #[schema(no_recursion)]
    Object(BTreeMap<String, Literal>),
}

/// Stored in the blueprint row, never in the workflow document or a timer node.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    #[default]
    Manual,
    Interval {
        seconds: u32,
    },
    Daily {
        time: String,
        timezone: String,
    },
}
