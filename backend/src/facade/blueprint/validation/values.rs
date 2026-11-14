use crate::facade::blueprint::{
    catalog::{NodeType, ValueType},
    definition::{Input, Literal, NodeKind, Operation, PathPart},
    operations::Operations,
    control_flow::ControlFlow,
    template::{self, Part},
};
use super::{
    graph::Graph,
    report::{Severity, ValidationReport},
};
use super::properties::Properties;
use crate::tools::base::{PropertyInfo, PropertyType};
use std::collections::BTreeMap;

pub(super) const MAX_INFERRED_SHAPES: usize = 65_536;
const MAX_INFERENCE_DEPTH: usize = 128;

#[derive(Clone)]
pub(super) struct Shape {
    pub kind: ValueType,
    pub items: Vec<Shape>,
    pub fields: BTreeMap<String, Shape>,
}
impl Shape {
    fn weight(&self) -> usize {
        1 + self.items.iter().map(Self::weight).sum::<usize>()
            + self.fields.values().map(Self::weight).sum::<usize>()
    }
    pub fn scalar(kind: ValueType) -> Self {
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
            _ => Self::scalar(ValueType::literal(value)),
        }
    }
}

pub(super) struct Values<'a, 'b> {
    pub graph: &'a Graph<'b>,
    pub report: &'a mut ValidationReport,
    pub node: usize,
    pub depth: usize,
    pub inferred: BTreeMap<usize, Shape>,
    pub remaining: usize,
}
impl<'a, 'b> Values<'a, 'b> {
    pub fn error(&mut self, field: &str, code: &str, message: impl Into<String>) {
        self.report.error(
            Some(&self.graph.workflow.nodes[self.node].id),
            field,
            code,
            message,
        );
    }
    pub fn infer(&mut self, input: &Input, field: &str) -> Shape {
        if self.depth >= MAX_INFERENCE_DEPTH {
            self.error(field, "binding_too_deep", "data dependency nesting exceeds 128 levels");
            return Shape::scalar(ValueType::Dynamic);
        }
        self.depth += 1;
        let shape = self.infer_inner(input, field);
        self.depth -= 1;
        shape
    }
    fn infer_inner(&mut self, input: &Input, field: &str) -> Shape {
        match input {
            Input::Literal { value } => Shape::literal(value),
            Input::Variable { node_id, path } => {
                match self.variable(node_id, field) {
                    Some(schema) => self.property_path(schema, path, field),
                    None => Shape::scalar(ValueType::Dynamic),
                }
            }
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
                    return Shape::scalar(ValueType::Dynamic);
                }
                let source_node = &self.graph.workflow.nodes[source];
                if matches!(source_node.kind, NodeKind::While { .. } | NodeKind::ForEach { .. }) && matches!(port.as_str(), "index" | "item") {
                    let roots = self.graph.workflow.edges.iter().filter(|edge| edge.from == source_node.id && edge.port == "body").map(|edge| edge.to.as_str());
                    if !ControlFlow::descendants(self.graph.workflow, roots).iter().any(|id| id == &self.graph.workflow.nodes[self.node].id) {
                        self.error(field, "loop_output_unavailable", "item/index is only available inside this loop body");
                        return Shape::scalar(ValueType::Dynamic);
                    }
                }
                match &self.graph.workflow.nodes[source].kind {
                    NodeKind::ForEach { items, .. } if port == "item" => {
                        let current = self.node;
                        self.node = source;
                        let list = self.infer(items, "kind.items");
                        self.node = current;
                        return self.shape_path(Self::item_shape(&list), path, field);
                    }
                    NodeKind::Variable { value_type, .. } if port == "value" => return self.property_path(value_type, path, field),
                    NodeKind::SetVariable { variable_id, .. } if port == "value" => {
                        let schema = self.graph.workflow.nodes.iter().find_map(|node| match &node.kind {
                            NodeKind::Variable { value_type, .. } if node.id == *variable_id => Some(value_type),
                            _ => None,
                        });
                        return schema.map(|schema| self.property_path(schema, path, field)).unwrap_or_else(|| Shape::scalar(ValueType::Dynamic));
                    }
                    NodeKind::Calculate { operation } if port == "value" => {
                        if let Some(shape) = self.inferred.get(&source).cloned() {
                            return self.shape_path(shape, path, field);
                        }
                        let source_node = self.node;
                        self.node = source;
                        let shape = self.operation(operation, "kind.operation");
                        self.node = source_node;
                        self.inferred.insert(source, shape.clone());
                        return self.shape_path(shape, path, field);
                    }
                    _ => {}
                }
                if let Some(registration) = self.graph.plugins.iter().find(|r| r.matches(&self.graph.workflow.nodes[source].kind)) {
                    return match registration.definition.outputs.iter().find(|p| &p.name == port) {
                        Some(property) => self.property_path(&property.property_type, path, field),
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
    pub fn variable(&mut self, id: &str, field: &str) -> Option<&'b PropertyType> {
        let Some(&source) = self.graph.indices.get(id) else {
            self.error(field, "variable_unavailable", "variable node does not exist");
            return None;
        };
        let NodeKind::Variable { value_type, .. } = &self.graph.workflow.nodes[source].kind else {
            self.error(field, "variable_unavailable", "reference must identify a Variable node");
            return None;
        };
        if !self.graph.available[self.node].contains(&source) {
            self.error(field, "variable_unavailable", "variable declaration must execute before this node");
        }
        Some(value_type)
    }
    fn property_path(&mut self, mut property: &PropertyType, path: &[PathPart], field: &str) -> Shape {
        for part in path {
            if matches!((property, part), (PropertyType::Object { properties }, PathPart::Field(_)) if properties.is_empty()) {
                return Shape::scalar(ValueType::Dynamic);
            }
            let next = match (property, part) {
                (PropertyType::Array { items }, PathPart::Index(_)) => Some(items.as_ref()),
                (PropertyType::Object { properties }, PathPart::Field(name)) => properties.iter().find(|p| &p.name == name).map(|p| &p.property_type),
                _ => None,
            };
            match next {
                Some(next) => property = next,
                None => {
                    self.error(field, "invalid_output_path", "value path is not declared by its type");
                    return Shape::scalar(ValueType::Dynamic);
                }
            }
        }
        Shape::property(property)
    }
    fn shape_path(&mut self, mut shape: Shape, path: &[PathPart], field: &str) -> Shape {
        for part in path {
            let next = match (shape.kind, part) {
                (ValueType::Dynamic, _) => return shape,
                (ValueType::Object, PathPart::Field(name)) => shape.fields.remove(name),
                (ValueType::Array, PathPart::Index(_)) => Some(Self::item_shape(&shape)),
                _ => None,
            };
            match next {
                Some(next) => shape = next,
                None => {
                    self.error(field, "invalid_output_path", "cannot select this path from the operation result");
                    return Shape::scalar(ValueType::Dynamic);
                }
            }
        }
        shape
    }
    fn item_shape(list: &Shape) -> Shape {
        match list.items.as_slice() {
            [item] => item.clone(),
            [first, rest @ ..] if !matches!(first.kind, ValueType::Array | ValueType::Object)
                && rest.iter().all(|item| item.kind == first.kind) => Shape::scalar(first.kind),
            _ => Shape::scalar(ValueType::Dynamic),
        }
    }
    pub fn operation(&mut self, operation: &Operation, field: &str) -> Shape {
        let mut inputs: Vec<_> = operation.inputs().iter().map(|(name, input)| self.infer(input, &format!("{field}.{name}"))).collect();
        let kinds: Vec<_> = inputs.iter().map(|shape| shape.kind).collect();
        let kind = match Operations::output_type(operation, &kinds) {
            Ok(kind) => kind,
            Err(error) => {
                self.error(field, "type_mismatch", error);
                return Shape::scalar(ValueType::Dynamic);
            }
        };
        let shape = match operation {
            Operation::Get { .. } => Self::item_shape(&inputs[0]),
            Operation::Append { .. } | Operation::Concat { .. } => {
                let right = inputs.pop().unwrap();
                let mut left = inputs.pop().unwrap();
                if left.kind == ValueType::Dynamic { left.items.push(Shape::scalar(ValueType::Dynamic)); }
                left.kind = kind;
                if matches!(operation, Operation::Append { .. }) { left.items.push(right); }
                else if right.kind == ValueType::Dynamic { left.items.push(Shape::scalar(ValueType::Dynamic)); }
                else { left.items.extend(right.items); }
                left
            }
            _ => Shape::scalar(kind),
        };
        // Repeated references can expand a small document exponentially during static inference too.
        if let Some(remaining) = self.remaining.checked_sub(shape.weight()) {
            self.remaining = remaining;
            shape
        } else {
            self.error(field, "binding_too_complex", "inferred data shapes exceed the validation budget");
            Shape::scalar(ValueType::Dynamic)
        }
    }
    pub fn property(&mut self, shape: &Shape, schema: &PropertyType, field: &str) {
        Properties { report: self.report, node_id: Some(&self.graph.workflow.nodes[self.node].id) }
            .property(shape, schema, field);
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
