mod graph;
mod properties;
mod readiness;
pub mod report;
mod values;

pub use readiness::Validation;

use std::collections::{BTreeMap, HashSet};
use crate::tools::base::{PropertyInfo, PropertyType};
use crate::plugins::blueprint::NodeRegistration;

use super::definition::{Input, Literal, ModelChoice, NodeKind, SCHEMA_VERSION, Schedule, Workflow};
use super::control_flow::MAX_LOOP_ITERATIONS;
use graph::Graph;
use report::{Diagnostic, ValidationReport};

pub const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_NODES: usize = 1000;
const MAX_EDGES: usize = 4000;

pub fn settings(name: &str, schedule: &Schedule) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 160 {
        return Err("a blueprint needs a name of up to 160 characters".into());
    }
    match schedule {
        Schedule::Interval { seconds: 0 } => {
            return Err("the interval must be at least one second".into());
        }
        Schedule::Daily { time, timezone } => {
            if time.len() != 5 || chrono::NaiveTime::parse_from_str(time, "%H:%M").is_err() {
                return Err("daily time must be HH:MM".into());
            }
            timezone
                .parse::<chrono_tz::Tz>()
                .map_err(|_| "unknown IANA timezone".to_string())?;
        }
        _ => {}
    }
    Ok(())
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}

/// Draft validation only: missing tools, models, entry nodes or incomplete branches do not prevent editing.
pub fn document(workflow: &Workflow) -> Result<(), Diagnostic> {
    if workflow.schema_version != SCHEMA_VERSION {
        return Err(Diagnostic::draft(
            None,
            "schema_version",
            format!(
                "unsupported workflow schema_version {}; expected {SCHEMA_VERSION}",
                workflow.schema_version
            ),
        ));
    }
    if workflow.nodes.len() > MAX_NODES || workflow.edges.len() > MAX_EDGES {
        return Err(Diagnostic::draft(
            None,
            "nodes",
            format!("at most {MAX_NODES} nodes and {MAX_EDGES} edges are allowed"),
        ));
    }
    let mut nodes = HashSet::new();
    for node in &workflow.nodes {
        if !identifier(&node.id) || !nodes.insert(node.id.as_str()) {
            return Err(Diagnostic::draft(
                Some(&node.id),
                "id",
                "invalid or duplicate node id",
            ));
        }
        if node.name.trim().is_empty()
            || node.name.chars().count() > 160
            || !node.position.x.is_finite()
            || !node.position.y.is_finite()
        {
            return Err(Diagnostic::draft(
                Some(&node.id),
                "name",
                "invalid name or position",
            ));
        }
        if node.size.as_ref().is_some_and(|size| {
            !(1.0..=4096.0).contains(&size.width) || !(1.0..=4096.0).contains(&size.height)
        }) {
            return Err(Diagnostic::draft(Some(&node.id), "size", "width and height must be between 1 and 4096"));
        }
    }
    let mut edges = HashSet::new();
    for (edge_index, edge) in workflow.edges.iter().enumerate() {
        if !identifier(&edge.id) || !edges.insert(edge.id.as_str()) || !identifier(&edge.port) {
            return Err(Diagnostic::draft(
                Some(&edge.from),
                &format!("edges[{edge_index}]"),
                "invalid or duplicate edge",
            ));
        }
        if !nodes.contains(edge.from.as_str()) || !nodes.contains(edge.to.as_str()) {
            return Err(Diagnostic::draft(
                Some(&edge.from),
                &format!("edges[{edge_index}]"),
                "edge references unknown node",
            ));
        }
    }
    for node in &workflow.nodes {
        if matches!(&node.kind, NodeKind::While { max_iterations, .. } | NodeKind::ForEach { max_iterations, .. }
            if *max_iterations == 0 || *max_iterations > MAX_LOOP_ITERATIONS) {
            return Err(Diagnostic::draft(Some(&node.id), "kind.max_iterations", format!("max_iterations must be 1..{MAX_LOOP_ITERATIONS}")));
        }
        match &node.kind {
            NodeKind::Merge { value_type, inputs } => {
                property_type(value_type, 0)
                    .map_err(|message| Diagnostic::draft(Some(&node.id), "kind.value_type", message))?;
                if inputs.keys().any(|id| !identifier(id)) {
                    return Err(Diagnostic::draft(Some(&node.id), "kind.inputs", "invalid incoming edge id"));
                }
            }
            NodeKind::Variable { value_type, initial, .. } => {
                property_type(value_type, 0)
                    .map_err(|message| Diagnostic::draft(Some(&node.id), "kind.value_type", message))?;
                variable_value(initial, value_type)
                    .map_err(|message| Diagnostic::draft(Some(&node.id), "kind.initial", message))?;
            }
            NodeKind::SetVariable { variable_id, .. } if !identifier(variable_id) => {
                return Err(Diagnostic::draft(Some(&node.id), "kind.variable_id", "invalid variable reference"));
            }
            NodeKind::Generate { model, .. } | NodeKind::Agent { model, .. } => {
                model_choice(model)
                    .map_err(|message| Diagnostic::draft(Some(&node.id), "kind.model", message))?;
            }
            NodeKind::Tool { name, .. } if !identifier(name) => {
                return Err(Diagnostic::draft(Some(&node.id), "kind", "invalid tool name"));
            }
            NodeKind::Switch { cases, .. } => {
                let mut ports = HashSet::new();
                for case in cases {
                    if !identifier(&case.port) || case.port == "default" || !ports.insert(&case.port) {
                        return Err(Diagnostic::draft(Some(&node.id), "kind.cases", "invalid or duplicate switch port"));
                    }
                }
            }
            NodeKind::Plugin { plugin_name, plugin_subname, node_type, version, .. }
                if !identifier(plugin_name) || !identifier(plugin_subname) || !identifier(node_type) || *version == 0 => {
                return Err(Diagnostic::draft(Some(&node.id), "kind", "invalid plugin node identity"));
            }
            _ => {}
        }
        if matches!(&node.kind, NodeKind::Agent { tools, .. } if tools.names.iter().any(|name| !identifier(name))) {
            return Err(Diagnostic::draft(Some(&node.id), "kind", "invalid tool name"));
        }
        for value in node.kind.inputs() {
            input(value, &nodes, 0)
                .map_err(|message| Diagnostic::draft(Some(&node.id), "kind", message))?;
        }
    }
    Ok(())
}

fn model_choice(model: &ModelChoice) -> Result<(), String> {
    if matches!(model, ModelChoice::Model { id, launch_profile_id } if *id <= 0 || launch_profile_id.is_some_and(|id| id <= 0))
    {
        return Err("model and launch profile ids must be positive".into());
    }
    Ok(())
}

fn input(value: &Input, nodes: &HashSet<&str>, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("input nesting is too deep".into());
    }
    match value {
        Input::Output { node_id, port, .. } => {
            if !nodes.contains(node_id.as_str()) || !identifier(port) {
                return Err(format!("invalid output reference '{node_id}.{port}'"));
            }
        }
        Input::Variable { node_id, .. } => {
            if !identifier(node_id) { return Err("invalid variable reference".into()); }
        }
        Input::Array { items } => {
            for item in items {
                input(item, nodes, depth + 1)?;
            }
        }
        Input::Object { fields } => {
            for item in fields.values() {
                input(item, nodes, depth + 1)?;
            }
        }
        Input::Literal { .. } => {}
    }
    Ok(())
}

fn property_type(kind: &PropertyType, depth: usize) -> Result<(), String> {
    if depth > 32 { return Err("variable type nesting is too deep".into()); }
    match kind {
        PropertyType::Array { items } => property_type(items, depth + 1)?,
        PropertyType::Object { properties } => {
            let mut names = HashSet::new();
            for property in properties {
                if !identifier(&property.name) || !names.insert(&property.name) {
                    return Err("invalid or duplicate variable object field".into());
                }
                property_type(&property.property_type, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn variable_value(value: &Literal, schema: &PropertyType) -> Result<(), String> {
    let mut report = ValidationReport::default();
    properties::Properties { report: &mut report, node_id: None }
        .property(&values::Shape::literal(value), schema, "value");
    match report.diagnostics.into_iter().next() {
        Some(error) => Err(error.message),
        None => Ok(()),
    }
}

pub(super) fn graph_errors(workflow: &Workflow, plugins: &[NodeRegistration]) -> ValidationReport {
    let mut report = ValidationReport::default();
    if let Err(error) = document(workflow) {
        report.diagnostics.push(error);
    } else {
        Graph::inspect(workflow, plugins, &mut report);
    }
    report.finish()
}

pub(super) fn tool_arguments(fields: &BTreeMap<String, Literal>, properties: &[PropertyInfo]) -> ValidationReport {
    let mut report = ValidationReport::default();
    let fields = fields.iter().map(|(name, value)| (name.clone(), values::Shape::literal(value))).collect();
    properties::Properties { report: &mut report, node_id: None }.properties(&fields, properties, "arguments");
    report.finish()
}

// Static and concrete plugin values use one checker; runtime values are literal shapes.
pub(super) fn plugin_values(fields: &BTreeMap<String, Literal>, schema: &[PropertyInfo]) -> Result<(), String> {
    if fields.keys().any(|name| !schema.iter().any(|p| &p.name == name)) {
        return Err("plugin node returned or received an undeclared field".into());
    }
    let report = tool_arguments(fields, schema);
    match report.diagnostics.into_iter().next() {
        Some(error) => Err(error.message),
        None => Ok(()),
    }
}
