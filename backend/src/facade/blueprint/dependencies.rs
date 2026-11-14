use std::collections::{HashMap, HashSet};

use super::definition::{Input, NodeKind, Workflow};

pub(super) struct Dependencies {
    pub sources: HashMap<String, HashSet<String>>,
    pub scopes: HashMap<String, Vec<String>>,
    pub merge_sources: HashMap<String, HashMap<String, HashSet<String>>>,
}
impl Dependencies {
    pub fn new(workflow: &Workflow) -> Self {
        let sources = workflow.nodes.iter().map(|node| {
            let mut sources = Self::references(node.kind.inputs());
            if let NodeKind::SetVariable { variable_id, .. } = &node.kind { sources.insert(variable_id.clone()); }
            (node.id.clone(), sources)
        }).collect();
        let merge_sources = workflow.nodes.iter().filter_map(|node| {
            if let NodeKind::Merge { inputs, .. } = &node.kind {
                Some((node.id.clone(), inputs.iter().map(|(id, input)| (id.clone(), Self::references(vec![input]))).collect()))
            } else { None }
        }).collect();
        let mut scopes = HashMap::new();
        let mut pending: Vec<_> = workflow.nodes.iter().filter(|node| matches!(node.kind, NodeKind::Entry))
            .map(|node| (node.id.clone(), vec![])).collect();
        while let Some((id, scope)) = pending.pop() {
            // Validation also uses this on incomplete drafts, before cycles/reachability are known.
            if scopes.contains_key(&id) { continue; }
            scopes.insert(id.clone(), scope.clone());
            let is_merge = workflow.nodes.iter().any(|node| node.id == id && matches!(node.kind, NodeKind::Merge { .. }));
            let is_loop = workflow.nodes.iter().any(|node| node.id == id && matches!(node.kind, NodeKind::While { .. } | NodeKind::ForEach { .. }));
            for edge in workflow.edges.iter().filter(|edge| edge.from == id) {
                let mut child = scope.clone();
                if is_merge || (is_loop && edge.port == "body") { child.push(id.clone()); }
                pending.push((edge.to.clone(), child));
            }
        }
        Self { sources, scopes, merge_sources }
    }

    pub fn references(mut inputs: Vec<&Input>) -> HashSet<String> {
        let mut sources = HashSet::new();
        while let Some(input) = inputs.pop() {
            match input {
                Input::Output { node_id, .. } | Input::Variable { node_id, .. } => { sources.insert(node_id.clone()); }
                Input::Array { items } => inputs.extend(items),
                Input::Object { fields } => inputs.extend(fields.values()),
                Input::Literal { .. } => {}
            }
        }
        sources
    }

    pub fn required(&self, node_id: &str, input: Option<&str>) -> &HashSet<String> {
        match input {
            Some(input) => &self.merge_sources[node_id][input],
            None => &self.sources[node_id],
        }
    }

    pub fn scope(&self, node_id: &str) -> &[String] {
        self.scopes.get(node_id).map(Vec::as_slice).unwrap_or_default()
    }
}
