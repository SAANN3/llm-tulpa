use std::collections::BTreeMap;

use super::{
    definition::{Literal, NodeKind, VariableScope, Workflow},
    runs::{NodeStatus, RunState},
    validation,
};

pub(crate) struct Variables;
impl Variables {
    pub fn restore(
        workflow: &Workflow,
        state: &mut RunState,
        saved: &BTreeMap<String, Literal>,
    ) -> Result<(), String> {
        let mut variables = state.variables.clone();
        for node in &workflow.nodes {
            if let NodeKind::Variable { value_type, scope: VariableScope::Blueprint, .. } = &node.kind {
                if let Some(value) = saved.get(&node.id) {
                    validation::variable_value(value, value_type)
                        .map_err(|error| format!("persistent variable '{}': {error}; reset its saved value or restore the compatible type", node.id))?;
                    variables.insert(node.id.clone(), value.clone());
                }
            }
        }
        let previous = std::mem::replace(&mut state.variables, variables);
        if !state.fits_budget().map_err(|error| error.to_string())? {
            state.variables = previous;
            return Err("persistent values make the run state exceed 4 MiB; reset saved values".into());
        }
        Ok(())
    }

    pub fn changed<'a>(workflow: &'a Workflow, state: &'a RunState, node_id: &str) -> Option<(&'a str, &'a Literal)> {
        if state.nodes.get(node_id)?.status != NodeStatus::Completed { return None; }
        let node = workflow.nodes.iter().find(|node| node.id == node_id)?;
        let variable_id = match &node.kind {
            NodeKind::Variable { .. } => node.id.as_str(),
            NodeKind::SetVariable { variable_id, .. } => variable_id.as_str(),
            _ => return None,
        };
        let variable = workflow.nodes.iter().find(|node| node.id == variable_id)?;
        if !matches!(&variable.kind, NodeKind::Variable { scope: VariableScope::Blueprint, .. }) { return None; }
        Some((variable_id, state.variables.get(variable_id)?))
    }
}
