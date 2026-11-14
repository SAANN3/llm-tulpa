use crate::facade::blueprint::{
    catalog::ValueType,
    definition::{Comparison, Input, Literal, ModelChoice, NodeKind, Workflow},
};
use super::{
    document, MAX_DOCUMENT_BYTES,
    graph::Graph,
    report::{Severity, ValidationReport},
    values::{MAX_INFERRED_SHAPES, Shape, Values, compatible, placeholders},
};
use crate::services::{
    error::ErrorService, launch_store::LaunchStore, model_store::ModelStore,
    tool_selection_store::ToolMode, tools::ToolService,
};
use std::{collections::BTreeMap, sync::Arc};

use crate::plugins::registry::PluginRegistry;

pub struct Validation {
    plugins: Arc<PluginRegistry>,
    tools: Arc<ToolService>,
    models: Arc<ModelStore>,
    profiles: Arc<LaunchStore>,
}
impl Validation {
    pub fn new(
        tools: Arc<ToolService>,
        models: Arc<ModelStore>,
        profiles: Arc<LaunchStore>,
        plugins: Arc<PluginRegistry>,
    ) -> Self {
        Self {
            plugins,
            tools,
            models,
            profiles,
        }
    }

    pub async fn check(
        &self,
        user_id: i64,
        workflow: &Workflow,
    ) -> Result<ValidationReport, ErrorService> {
        let mut report = ValidationReport::default();
        if let Err(diagnostic) = document(workflow) {
            report.diagnostics.push(diagnostic);
            return Ok(report.finish());
        }
        let bytes = serde_json::to_vec_pretty(workflow)
            .map_err(|_| ErrorService::internal("could not serialize workflow for validation"))?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            report.error(
                None,
                "workflow",
                "document_too_large",
                "workflow exceeds 1 MiB",
            );
            return Ok(report.finish());
        }
        let plugins = self.plugins.blueprint_nodes().await;
        let graph = Graph::inspect(workflow, &plugins, &mut report);
        let tools = self.tools.catalog(user_id).await;
        let choices: Vec<_> = workflow
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                NodeKind::Generate { model, .. } | NodeKind::Agent { model, .. } => Some(model),
                _ => None,
            })
            .collect();
        let ids: Vec<_> = choices
            .iter()
            .filter_map(|m| match m {
                ModelChoice::Model { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        let models = self.models.get_many(&ids).await?;
        let needs_default = choices
            .iter()
            .any(|m| matches!(m, ModelChoice::UserDefault));
        let default = if needs_default {
            self.models.default_for_user(user_id).await?
        } else {
            None
        };
        let default_profile = if needs_default {
            self.models.chosen_profile(user_id).await?
        } else {
            None
        };
        let needs_profiles = default_profile.is_some()
            || choices.iter().any(|m| {
                matches!(
                    m,
                    ModelChoice::Model {
                        launch_profile_id: Some(_),
                        ..
                    }
                )
            });
        let profiles = if needs_profiles {
            self.profiles.list(None).await?
        } else {
            vec![]
        };
        for (index, node) in workflow.nodes.iter().enumerate() {
            let mut values = Values {
                graph: &graph,
                report: &mut report,
                node: index,
                depth: 0,
                inferred: BTreeMap::new(),
                remaining: MAX_INFERRED_SHAPES,
            };
            match &node.kind {
                NodeKind::Generate { model, prompt, .. }
                | NodeKind::Agent { model, prompt, .. } => {
                    let (resolved, profile) = match model {
                        ModelChoice::UserDefault => (default.as_ref(), default_profile),
                        ModelChoice::Model {
                            id,
                            launch_profile_id,
                        } => (models.get(id), *launch_profile_id),
                    };
                    if let Some(model) = resolved {
                        if let Some(profile) = profile {
                            if !profiles
                                .iter()
                                .any(|p| p.id == profile && p.model_id == model.id)
                            {
                                values.error(
                                    "kind.model.launch_profile_id",
                                    "profile_unavailable",
                                    "launch profile is missing or belongs to another model",
                                );
                            }
                        }
                    } else {
                        values.error(
                            "kind.model",
                            "model_unavailable",
                            "no registered model resolves this selection",
                        );
                    }
                    let shape = values.infer(prompt, "kind.prompt");
                    values.expect(&shape, ValueType::String, "kind.prompt");
                    if let NodeKind::Agent {
                        system_prompt,
                        tools: selection,
                        ..
                    } = &node.kind
                    {
                        if let Some(input) = system_prompt {
                            let shape = values.infer(input, "kind.system_prompt");
                            values.expect(&shape, ValueType::String, "kind.system_prompt");
                        }
                        for (i, name) in selection.names.iter().enumerate() {
                            if !tools.iter().any(|t| t.name == *name) {
                                let severity = if matches!(selection.mode, ToolMode::Whitelist) {
                                    Severity::Error
                                } else {
                                    Severity::Warning
                                };
                                values.report.issue(
                                    severity,
                                    Some(&node.id),
                                    &format!("kind.tools.names[{i}]"),
                                    "tool_unavailable",
                                    format!(
                                        "tool '{name}' is not currently published for this user"
                                    ),
                                );
                            }
                        }
                    }
                }
                NodeKind::Tool { name, arguments, .. } => {
                    let fields: BTreeMap<_, _> = arguments
                        .iter()
                        .map(|(k, v)| (k.clone(), values.infer(v, &format!("kind.arguments.{k}"))))
                        .collect();
                    if let Some(tool) = tools.iter().find(|t| t.name == *name) {
                        values.properties(&fields, &tool.properties, "kind.arguments");
                        if tool.properties.is_empty() && !fields.is_empty() {
                            values.error(
                                "kind.arguments",
                                "unknown_argument",
                                "this tool takes no arguments",
                            );
                        }
                    } else {
                        values.error(
                            "kind.name",
                            "tool_unavailable",
                            format!("tool '{name}' is not currently published for this user"),
                        );
                    }
                }
                NodeKind::Sleep { seconds } => {
                    let shape = values.infer(seconds, "kind.seconds");
                    values.expect(&shape, ValueType::Number, "kind.seconds");
                    if matches!(seconds, Input::Literal {value: Literal::Integer(v)} if *v < 0)
                        || matches!(seconds, Input::Literal {value: Literal::Number(v)} if !v.is_finite() || *v < 0.0)
                    {
                        values.error(
                            "kind.seconds",
                            "invalid_duration",
                            "sleep duration must be finite and nonnegative",
                        );
                    }
                }
                NodeKind::If {
                    left,
                    operator,
                    right,
                } | NodeKind::While {
                    left,
                    operator,
                    right,
                    ..
                } => {
                    let left = values.infer(left, "kind.left");
                    let right = values.infer(right, "kind.right");
                    match operator {
                        Comparison::Greater
                        | Comparison::GreaterOrEqual
                        | Comparison::Less
                        | Comparison::LessOrEqual => {
                            values.expect(&left, ValueType::Number, "kind.left");
                            values.expect(&right, ValueType::Number, "kind.right");
                        }
                        Comparison::Contains => {
                            values.expect(&left, ValueType::String, "kind.left");
                            values.expect(&right, ValueType::String, "kind.right");
                        }
                        _ => {
                            if !compatible(left.kind, right.kind)
                                && !compatible(right.kind, left.kind)
                            {
                                values.error(
                                    "kind.right",
                                    "type_mismatch",
                                    "comparison operands have incompatible types",
                                );
                            }
                        }
                    }
                }
                NodeKind::Switch { value, cases } => {
                    let shape = values.infer(value, "kind.value");
                    for (i, case) in cases.iter().enumerate() {
                        let kind = Shape::literal(&case.value).kind;
                        if !compatible(shape.kind, kind) && !compatible(kind, shape.kind) {
                            values.error(
                                &format!("kind.cases[{i}].value"),
                                "type_mismatch",
                                "case and switch value have incompatible types",
                            );
                        }
                    }
                }
                NodeKind::Format {
                    template,
                    arguments,
                } => {
                    for (name, input) in arguments {
                        values.infer(input, &format!("kind.arguments.{name}"));
                    }
                    match placeholders(template) {
                        Ok(names) => {
                            for name in names {
                                if !arguments.contains_key(&name) {
                                    values.error(
                                        &format!("kind.arguments.{name}"),
                                        "missing_argument",
                                        "template placeholder has no binding",
                                    );
                                }
                            }
                        }
                        Err(message) => values.error("kind.template", "invalid_template", message),
                    }
                }
                NodeKind::Return { values: inputs } => {
                    for (name, input) in inputs { values.infer(input, &format!("kind.values.{name}")); }
                }
                NodeKind::Plugin { arguments, .. } => {
                    let fields = arguments.iter().map(|(name,input)| (name.clone(), values.infer(input, &format!("kind.arguments.{name}")))).collect();
                    if let Some(registration) = plugins.iter().find(|r| r.matches(&node.kind)) {
                        values.properties(&fields, &registration.definition.inputs, "kind.arguments");
                        for name in arguments.keys().filter(|name| !registration.definition.inputs.iter().any(|p| &p.name == *name)) {
                            values.error(&format!("kind.arguments.{name}"), "unknown_argument", "argument is not declared by the plugin node");
                        }
                    } else {
                        values.error("kind", "plugin_node_unavailable", "plugin node/version is not available from an enabled plugin");
                    }
                }
                NodeKind::Merge { value_type, inputs } => {
                    for (edge_id, input) in inputs {
                        let field = format!("kind.inputs.{edge_id}");
                        let shape = values.infer(input, &field);
                        values.property(&shape, value_type, &field);
                    }
                }
                NodeKind::Entry => {}
                NodeKind::Variable { .. } => {}
                NodeKind::SetVariable { variable_id, value } => {
                    let shape = values.infer(value, "kind.value");
                    if let Some(schema) = values.variable(variable_id, "kind.variable_id") {
                        values.property(&shape, schema, "kind.value");
                    }
                }
                NodeKind::Calculate { operation } => { values.operation(operation, "kind.operation"); }
                NodeKind::ForEach { items, .. } => {
                    let shape = values.infer(items, "kind.items");
                    values.expect(&shape, ValueType::Array, "kind.items");
                }
            }
        }
        Ok(report.finish())
    }
}
