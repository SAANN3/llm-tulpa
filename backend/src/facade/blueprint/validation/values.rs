use crate::facade::blueprint::{
    catalog::{NodeType, ValueType},
    definition::{Input, Literal, PathPart},
    template::{self, Part},
};
use super::{
    graph::Graph,
    report::{Severity, ValidationReport},
};
use super::properties::Properties;
use crate::tools::base::{PropertyInfo, PropertyType};
use std::collections::BTreeMap;

pub(super) struct Shape {
    pub kind: ValueType,
    pub items: Vec<Shape>,
    pub fields: BTreeMap<String, Shape>,
}
impl Shape {
    fn scalar(kind: ValueType) -> Self {
        Self {
            kind,
            items: vec![],
            fields: BTreeMap::new(),
        }
    }
    pub fn property(value: &PropertyType) -> Self {
        match value {
            PropertyType::Object { properties } if properties.is_empty() => Self::scalar(ValueType::Dynamic),
            PropertyType::Object { properties } => Self {
                kind: ValueType::Object, items: vec![], fields: properties.iter().map(|p| (p.name.clone(), Self::property(&p.property_type))).collect(),
            },
            PropertyType::Array { items } => Self { kind: ValueType::Array, items: vec![Self::property(items)], fields: BTreeMap::new() },
            _ => Self::scalar(ValueType::from(value)),
        }
    }
    pub fn literal(value: &Literal) -> Self {
        match value {
            Literal::Null => Self::scalar(ValueType::Null),
            Literal::Boolean(_) => Self::scalar(ValueType::Boolean),
            Literal::Integer(_) => Self::scalar(ValueType::Integer),
            Literal::Number(_) => Self::scalar(ValueType::Number),
            Literal::String(_) => Self::scalar(ValueType::String),
            Literal::Array(items) => Self {
                kind: ValueType::Array,
                items: items.iter().map(Self::literal).collect(),
                fields: BTreeMap::new(),
            },
            Literal::Object(fields) => Self {
                kind: ValueType::Object,
                items: vec![],
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::literal(v)))
                    .collect(),
            },
        }
    }
}

pub(super) struct Values<'a, 'b> {
    pub graph: &'a Graph<'b>,
    pub report: &'a mut ValidationReport,
    pub node: usize,
}
impl Values<'_, '_> {
    pub fn error(&mut self, field: &str, code: &str, message: impl Into<String>) {
        self.report.error(
            Some(&self.graph.workflow.nodes[self.node].id),
            field,
            code,
            message,
        );
    }
    pub fn infer(&mut self, input: &Input, field: &str) -> Shape {
        match input {
            Input::Literal { value } => Shape::literal(value),
            Input::Array { items } => Shape {
                kind: ValueType::Array,
                items: items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| self.infer(v, &format!("{field}.items[{i}]")))
                    .collect(),
                fields: BTreeMap::new(),
            },
            Input::Object { fields } => Shape {
                kind: ValueType::Object,
                items: vec![],
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.infer(v, &format!("{field}.fields.{k}"))))
                    .collect(),
            },
            Input::Output {
                node_id,
                port,
                path,
            } => {
                let source = self.graph.indices[node_id.as_str()];
                if !self.graph.available[self.node].contains(&source) {
                    self.error(
                        field,
                        "output_unavailable",
                        format!("'{node_id}' is not guaranteed to finish before this node"),
                    );
                }
                if let Some(registration) = self.graph.plugins.iter().find(|r| r.matches(&self.graph.workflow.nodes[source].kind)) {
                    let mut property = registration.definition.outputs.iter().find(|p| &p.name == port).map(|p| &p.property_type);
                    for part in path {
                        if matches!((property, part), (Some(PropertyType::Object { properties }), PathPart::Field(_)) if properties.is_empty()) {
                            return Shape::scalar(ValueType::Dynamic);
                        }
                        property = match (property, part) {
                            (Some(PropertyType::Array { items }), PathPart::Index(_)) => Some(items),
                            (Some(PropertyType::Object { properties }), PathPart::Field(name)) => properties.iter().find(|p| &p.name == name).map(|p| &p.property_type),
                            _ => None,
                        };
                    }
                    return match property {
                        Some(property) => Shape::property(property),
                        None => {
                            self.error(field, "invalid_output_path", "plugin output port/path is not declared");
                            Shape::scalar(ValueType::Dynamic)
                        }
                    };
                }
                let Some(kind) = NodeType::output(&self.graph.workflow.nodes[source].kind, port)
                else {
                    self.error(
                        field,
                        "unknown_data_port",
                        format!("'{node_id}' has no data output '{port}'"),
                    );
                    return Shape::scalar(ValueType::Dynamic);
                };
                if kind == ValueType::Dynamic {
                    self.report.issue(Severity::Warning, Some(&self.graph.workflow.nodes[self.node].id), field, "runtime_type_check", "tool result schema is unknown; check the selected path and value type during execution");
                } else if !path.is_empty() {
                    // Current statically typed output ports are scalars. Unknown tool results stay unknown.
                    let selection = match &path[0] {
                        PathPart::Field(k) => k.clone(),
                        PathPart::Index(i) => i.to_string(),
                    };
                    self.error(
                        field,
                        "invalid_output_path",
                        format!("cannot select '{selection}' from {kind:?}"),
                    );
                }
                Shape::scalar(kind)
            }
        }
    }
    pub fn expect(&mut self, shape: &Shape, expected: ValueType, field: &str) {
        if !compatible(shape.kind, expected) {
            self.error(
                field,
                "type_mismatch",
                format!("expected {expected:?}, got {:?}", shape.kind),
            );
        }
    }
    pub fn properties(
        &mut self,
        fields: &BTreeMap<String, Shape>,
        properties: &[PropertyInfo],
        field: &str,
    ) {
        Properties {
            report: self.report,
            node_id: Some(&self.graph.workflow.nodes[self.node].id),
        }
        .properties(fields, properties, field);
    }

}

pub(super) fn compatible(actual: ValueType, expected: ValueType) -> bool {
    actual == expected
        || actual == ValueType::Dynamic
        || expected == ValueType::Dynamic
        || actual == ValueType::Integer && expected == ValueType::Number
}

pub(super) fn placeholders(template: &str) -> Result<Vec<String>, String> {
    Ok(template::parse(template)?
        .into_iter()
        .filter_map(|part| match part {
            Part::Argument(name) => Some(name.to_owned()),
            Part::Text(_) => None,
        })
        .collect())
}
