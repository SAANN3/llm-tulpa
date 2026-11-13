mod graph;
mod readiness;
pub mod report;
mod values;

pub use readiness::Validation;

use std::collections::HashSet;

use super::definition::{Input, ModelChoice, NodeKind, SCHEMA_VERSION, Schedule, Workflow};
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
        let inputs: Vec<&Input> = match &node.kind {
            NodeKind::Entry => vec![],
            NodeKind::Generate { model, prompt, .. } => {
                model_choice(model)
                    .map_err(|message| Diagnostic::draft(Some(&node.id), "kind.model", message))?;
                vec![prompt]
            }
            NodeKind::Agent {
                model,
                prompt,
                system_prompt,
                tools,
                ..
            } => {
                model_choice(model)
                    .map_err(|message| Diagnostic::draft(Some(&node.id), "kind.model", message))?;
                if tools.names.iter().any(|n| !identifier(n)) {
                    return Err(Diagnostic::draft(
                        Some(&node.id),
                        "kind",
                        "invalid tool name",
                    ));
                }
                std::iter::once(prompt)
                    .chain(system_prompt.iter())
                    .collect()
            }
            NodeKind::Tool { name, arguments } => {
                if !identifier(name) {
                    return Err(Diagnostic::draft(
                        Some(&node.id),
                        "kind",
                        "invalid tool name",
                    ));
                }
                arguments.values().collect()
            }
            NodeKind::Sleep { seconds } => vec![seconds],
            NodeKind::If { left, right, .. } => vec![left, right],
            NodeKind::Switch { value, cases } => {
                let mut ports = HashSet::new();
                for case in cases {
                    if !identifier(&case.port)
                        || case.port == "default"
                        || !ports.insert(&case.port)
                    {
                        return Err(Diagnostic::draft(
                            Some(&node.id),
                            "kind.cases",
                            "invalid or duplicate switch port",
                        ));
                    }
                }
                vec![value]
            }
            NodeKind::Format { arguments, .. } => arguments.values().collect(),
            NodeKind::Return { values } => values.values().collect(),
            NodeKind::Plugin {
                plugin_name,
                plugin_subname,
                node_type,
                version,
                arguments,
            } => {
                if !identifier(plugin_name)
                    || !identifier(plugin_subname)
                    || !identifier(node_type)
                    || *version == 0
                {
                    return Err(Diagnostic::draft(
                        Some(&node.id),
                        "kind",
                        "invalid plugin node identity",
                    ));
                }
                arguments.values().collect()
            }
        };
        for value in inputs {
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

pub(super) fn graph_errors(workflow: &Workflow) -> ValidationReport {
    let mut report = ValidationReport::default();
    if let Err(error) = document(workflow) {
        report.diagnostics.push(error);
    } else {
        Graph::inspect(workflow, &mut report);
    }
    report.finish()
}
