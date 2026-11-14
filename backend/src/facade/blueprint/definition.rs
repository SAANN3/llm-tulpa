use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::{llm::ThinkChoice, tool_selection_store::ToolMode};
use crate::tools::base::PropertyType;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariableScope {
    #[default]
    Run,
    Blueprint,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Entry,
    Variable {
        value_type: PropertyType,
        initial: Literal,
        #[serde(default)]
        scope: VariableScope,
    },
    SetVariable {
        variable_id: String,
        value: Input,
    },
    Calculate {
        operation: Operation,
    },
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
    While {
        left: Input,
        operator: Comparison,
        right: Input,
        #[serde(default = "default_iterations")]
        max_iterations: u32,
    },
    ForEach {
        items: Input,
        #[serde(default = "default_iterations")]
        max_iterations: u32,
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

fn default_iterations() -> u32 { 1000 }

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
    Variable {
        node_id: String,
        #[serde(default)]
        path: Vec<PathPart>,
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
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Add { left: Input, right: Input },
    Subtract { left: Input, right: Input },
    Multiply { left: Input, right: Input },
    Divide { left: Input, right: Input },
    Remainder { left: Input, right: Input },
    And { left: Input, right: Input },
    Or { left: Input, right: Input },
    Not { value: Input },
    Length { value: Input },
    Get { list: Input, index: Input },
    Append { list: Input, value: Input },
    Concat { left: Input, right: Input },
}

impl NodeKind {
    pub(super) fn inputs(&self) -> Vec<&Input> {
        match self {
            Self::Entry | Self::Variable { .. } => vec![],
            Self::SetVariable { value, .. } | Self::Switch { value, .. } => vec![value],
            Self::Calculate { operation } => operation.inputs().into_iter().map(|(_, input)| input).collect(),
            Self::Generate { prompt, .. } => vec![prompt],
            Self::Agent { prompt, system_prompt, .. } => std::iter::once(prompt).chain(system_prompt.iter()).collect(),
            Self::Tool { arguments, .. } | Self::Format { arguments, .. } | Self::Plugin { arguments, .. } => arguments.values().collect(),
            Self::Sleep { seconds } => vec![seconds],
            Self::If { left, right, .. } | Self::While { left, right, .. } => vec![left, right],
            Self::ForEach { items, .. } => vec![items],
            Self::Return { values } => values.values().collect(),
        }
    }
}

impl Operation {
    pub(super) fn inputs(&self) -> Vec<(&'static str, &Input)> {
        match self {
            Self::Add { left, right } | Self::Subtract { left, right }
            | Self::Multiply { left, right } | Self::Divide { left, right }
            | Self::Remainder { left, right } | Self::And { left, right }
            | Self::Or { left, right } | Self::Concat { left, right } => vec![("left", left), ("right", right)],
            Self::Not { value } | Self::Length { value } => vec![("value", value)],
            Self::Get { list, index } => vec![("list", list), ("index", index)],
            Self::Append { list, value } => vec![("list", list), ("value", value)],
        }
    }
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
