use serde::Serialize;
use utoipa::ToSchema;

use super::definition::{Literal, NodeKind};
use crate::plugins::blueprint::NodeRegistration;
use crate::tools::base::PropertyType;

/// Data ports and control ports are separate: an output value does not schedule a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
    Dynamic,
}

#[derive(Serialize, ToSchema)]
pub struct OutputPort {
    pub name: &'static str,
    pub value_type: ValueType,
}

#[derive(Serialize, ToSchema)]
pub struct NodeDescriptor {
    pub kind: NodeType,
    pub title: &'static str,
    pub control_outputs: Vec<&'static str>,
    pub outputs: Vec<OutputPort>,
    pub dynamic_control_outputs: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Entry,
    Variable,
    SetVariable,
    Calculate,
    Generate,
    Agent,
    Tool,
    Sleep,
    If,
    While,
    ForEach,
    Switch,
    Format,
    Return,
    Plugin,
}

impl NodeType {
    pub fn builtins() -> Vec<NodeDescriptor> {
        [
            Self::Entry,
            Self::Variable,
            Self::SetVariable,
            Self::Calculate,
            Self::Generate,
            Self::Agent,
            Self::Tool,
            Self::Sleep,
            Self::If,
            Self::While,
            Self::ForEach,
            Self::Switch,
            Self::Format,
            Self::Return,
        ]
        .into_iter()
        .map(Self::descriptor)
        .collect()
    }

    pub(super) fn kind(node: &NodeKind) -> Self {
        match node {
            NodeKind::Entry => Self::Entry,
            NodeKind::Variable { .. } => Self::Variable,
            NodeKind::SetVariable { .. } => Self::SetVariable,
            NodeKind::Calculate { .. } => Self::Calculate,
            NodeKind::Generate { .. } => Self::Generate,
            NodeKind::Agent { .. } => Self::Agent,
            NodeKind::Tool { .. } => Self::Tool,
            NodeKind::Sleep { .. } => Self::Sleep,
            NodeKind::If { .. } => Self::If,
            NodeKind::While { .. } => Self::While,
            NodeKind::ForEach { .. } => Self::ForEach,
            NodeKind::Switch { .. } => Self::Switch,
            NodeKind::Format { .. } => Self::Format,
            NodeKind::Return { .. } => Self::Return,
            NodeKind::Plugin { .. } => Self::Plugin,
        }
    }

    pub(super) fn descriptor(self) -> NodeDescriptor {
        let (title, control_outputs, output) = match self {
            Self::Entry => ("Start", vec!["out"], None),
            Self::Variable => ("Variable", vec!["out"], Some(("value", ValueType::Dynamic))),
            Self::SetVariable => ("Set variable", vec!["out"], Some(("value", ValueType::Dynamic))),
            Self::Calculate => ("Calculate", vec!["out"], Some(("value", ValueType::Dynamic))),
            Self::Generate => ("Generate", vec!["out"], Some(("text", ValueType::String))),
            Self::Agent => ("Agent", vec!["out"], Some(("text", ValueType::String))),
            Self::Tool => ("Tool", vec!["out"], Some(("result", ValueType::Dynamic))),
            Self::Sleep => ("Sleep", vec!["out"], None),
            Self::If => ("If", vec!["true", "false"], None),
            Self::While => ("While", vec!["body", "out"], None),
            Self::ForEach => ("For each", vec!["body", "out"], None),
            Self::Switch => ("Switch", vec!["default"], None),
            Self::Format => ("Format", vec!["out"], Some(("text", ValueType::String))),
            Self::Return => ("Return", vec![], None),
            Self::Plugin => ("Plugin", vec![], None),
        };
        let mut outputs: Vec<_> = output.into_iter().map(|(name, value_type)| OutputPort { name, value_type }).collect();
        if matches!(self, Self::While | Self::ForEach) {
            outputs.extend([
                OutputPort { name: "index", value_type: ValueType::Integer },
                OutputPort { name: "iterations", value_type: ValueType::Integer },
            ]);
        }
        if self == Self::ForEach { outputs.push(OutputPort { name: "item", value_type: ValueType::Dynamic }); }
        NodeDescriptor {
            kind: self,
            title,
            control_outputs,
            outputs,
            dynamic_control_outputs: self == Self::Switch,
        }
    }

    pub(super) fn control_ports(node: &NodeKind, plugins: &[NodeRegistration]) -> Vec<String> {
        if matches!(node, NodeKind::Plugin { .. }) {
            return plugins.iter().find(|r| r.matches(node)).map(|r| r.definition.control_outputs.clone()).unwrap_or_default();
        }
        let mut ports: Vec<_> = Self::kind(node)
            .descriptor()
            .control_outputs
            .into_iter()
            .map(str::to_owned)
            .collect();
        if let NodeKind::Switch { cases, .. } = node {
            ports.extend(cases.iter().map(|case| case.port.clone()));
        }
        ports
    }

    pub(super) fn output(node: &NodeKind, port: &str) -> Option<ValueType> {
        Self::kind(node)
            .descriptor()
            .outputs
            .into_iter()
            .find(|p| p.name == port)
            .map(|p| p.value_type)
    }
}

impl ValueType {
    pub(super) fn literal(value: &Literal) -> Self {
        match value {
            Literal::Null => Self::Null,
            Literal::Boolean(_) => Self::Boolean,
            Literal::Integer(_) => Self::Integer,
            Literal::Number(_) => Self::Number,
            Literal::String(_) => Self::String,
            Literal::Array(_) => Self::Array,
            Literal::Object(_) => Self::Object,
        }
    }
}

impl From<&PropertyType> for ValueType {
    fn from(value: &PropertyType) -> Self {
        match value {
            PropertyType::String => Self::String,
            PropertyType::Number => Self::Number,
            PropertyType::Integer => Self::Integer,
            PropertyType::Boolean => Self::Boolean,
            PropertyType::Array { .. } => Self::Array,
            PropertyType::Object { .. } => Self::Object,
        }
    }
}
