use crate::facade::blueprint::{
    catalog::NodeType,
    definition::{NodeKind, Workflow},
    control_flow::{ControlFlow, MAX_LOOP_DEPTH},
};
use super::report::ValidationReport;
use crate::plugins::blueprint::NodeRegistration;
use std::collections::{HashMap, HashSet};

pub(super) struct Graph<'a> {
    pub workflow: &'a Workflow,
    pub plugins: &'a [NodeRegistration],
    pub indices: HashMap<&'a str, usize>,
    /// Nodes guaranteed to have finished before a node, including earlier unconditional branches.
    pub available: Vec<HashSet<usize>>,
}

impl<'a> Graph<'a> {
    pub fn inspect(workflow: &'a Workflow, plugins: &'a [NodeRegistration], report: &mut ValidationReport) -> Self {
        let indices: HashMap<_, _> = workflow
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let n = workflow.nodes.len();
        let mut graph = Self {
            workflow,
            plugins,
            indices,
            available: vec![HashSet::new(); n],
        };
        let mut incoming = vec![0usize; n];
        let mut outgoing = vec![Vec::new(); n];
        let mut valid = true;
        for (i, edge) in workflow.edges.iter().enumerate() {
            let from = graph.indices[edge.from.as_str()];
            let to = graph.indices[edge.to.as_str()];
            if !NodeType::control_ports(&workflow.nodes[from].kind, plugins).contains(&edge.port) {
                report.error(
                    Some(&edge.from),
                    &format!("edges[{i}].port"),
                    "unknown_control_port",
                    format!("unknown control output '{}'", edge.port),
                );
                valid = false;
            }
            incoming[to] += 1;
            outgoing[from].push((edge.port.as_str(), to));
        }
        let entries: Vec<_> = workflow
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Entry))
            .map(|(i, _)| i)
            .collect();
        if entries.len() != 1 {
            report.error(
                None,
                "nodes",
                "entry_count",
                "exactly one entry node is required",
            );
            valid = false;
        }
        for (i, count) in incoming.iter().enumerate() {
            if *count > 1 {
                report.error(Some(&workflow.nodes[i].id), "edges", "merge_unsupported", "multiple incoming edges require explicit merge semantics, which are not supported yet");
                valid = false;
            }
            if *count > 0 && matches!(workflow.nodes[i].kind, NodeKind::Entry) {
                report.error(
                    Some(&workflow.nodes[i].id),
                    "edges",
                    "entry_has_input",
                    "entry cannot have incoming control edges",
                );
                valid = false;
            }
        }
        // Kahn also finds cycles in disconnected components before computing output availability.
        let mut degrees = incoming;
        let mut queue: Vec<_> = (0..n).filter(|i| degrees[*i] == 0).collect();
        let mut processed = 0;
        while let Some(i) = queue.pop() {
            processed += 1;
            for (_, to) in &outgoing[i] {
                degrees[*to] -= 1;
                if degrees[*to] == 0 {
                    queue.push(*to);
                }
            }
        }
        if processed != n {
            report.error(None, "edges", "cycle", "control graph contains a cycle");
            valid = false;
        }
        if !valid {
            return graph;
        }
        let mut depths = vec![0; n];
        for node in workflow.nodes.iter().filter(|node| matches!(node.kind, NodeKind::While { .. } | NodeKind::ForEach { .. })) {
            let roots = workflow.edges.iter().filter(|edge| edge.from == node.id && edge.port == "body").map(|edge| edge.to.as_str());
            for member in ControlFlow::descendants(workflow, roots) { depths[graph.indices[member.as_str()]] += 1; }
        }
        if depths.iter().any(|depth| *depth > MAX_LOOP_DEPTH) {
            report.error(None, "edges", "loop_nesting", "loop nesting exceeds 32 levels");
            return graph;
        }
        let mut visited = HashSet::new();
        graph.visit(entries[0], HashSet::new(), &outgoing, &mut visited);
        for (i, node) in workflow.nodes.iter().enumerate() {
            if !visited.contains(&i) {
                report.error(
                    Some(&node.id),
                    "edges",
                    "unreachable",
                    "node is not reachable from entry",
                );
            }
        }
        graph
    }

    fn visit(
        &mut self,
        entry: usize,
        mut available: HashSet<usize>,
        outgoing: &[Vec<(&str, usize)>],
        visited: &mut HashSet<usize>,
    ) {
        enum Step {
            Visit(usize),
            Restore(HashSet<usize>),
        }
        let mut pending = vec![Step::Visit(entry)];
        // A user graph can be a thousand nodes deep; keep traversal off the call stack.
        while let Some(step) = pending.pop() {
            let i = match step {
                Step::Restore(before) => {
                    available = before;
                    continue;
                }
                Step::Visit(i) => i,
            };
            visited.insert(i);
            self.available[i] = available.clone();
            available.insert(i);
            if matches!(
                self.workflow.nodes[i].kind,
                NodeKind::If { .. } | NodeKind::Switch { .. } | NodeKind::Plugin { .. }
                    | NodeKind::While { .. } | NodeKind::ForEach { .. }
            ) {
                // Without merges, distinct outcomes cannot guarantee any shared descendant.
                // Each outcome starts with the same available values and restores them afterwards.
                pending.push(Step::Restore(available.clone()));
                for port in NodeType::control_ports(&self.workflow.nodes[i].kind, self.plugins)
                    .into_iter()
                    .rev()
                {
                    for (_, to) in outgoing[i].iter().rev().filter(|(p, _)| *p == port) {
                        pending.push(Step::Visit(*to));
                    }
                    pending.push(Step::Restore(available.clone()));
                }
            } else {
                for (_, to) in outgoing[i].iter().rev() {
                    pending.push(Step::Visit(*to));
                }
            }
        }
    }
}
