use crate::facade::blueprint::{
    catalog::NodeType,
    definition::{NodeKind, Workflow},
    control_flow::{ControlFlow, MAX_LOOP_DEPTH},
    dependencies::Dependencies,
};
use super::report::ValidationReport;
use crate::plugins::blueprint::NodeRegistration;
use std::collections::{HashMap, HashSet};

pub(super) struct Graph<'a> {
    pub workflow: &'a Workflow,
    pub plugins: &'a [NodeRegistration],
    pub indices: HashMap<&'a str, usize>,
    /// Required control/data predecessors; runtime waits for their current results.
    pub available: Vec<HashSet<usize>>,
    pub dependencies: Dependencies,
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
            dependencies: Dependencies::new(workflow),
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
        if !valid { return graph; }
        if graph.dependencies.scopes.values().any(|scope| scope.len() > MAX_LOOP_DEPTH) {
            report.error(None, "edges", "loop_nesting", "loop nesting exceeds 32 levels");
            return graph;
        }
        let visited: HashSet<_> = ControlFlow::descendants(workflow, std::iter::once(workflow.nodes[entries[0]].id.as_str())).into_iter().collect();
        for node in &workflow.nodes {
            if !visited.contains(&node.id) {
                report.error(Some(&node.id), "edges", "unreachable", "node is not reachable from entry");
            }
        }
        for (id, sources) in &graph.dependencies.sources {
            let to = graph.indices[id.as_str()];
            for source in sources {
                let Some(&from) = graph.indices.get(source.as_str()) else { continue };
                if !graph.dependencies.scope(id).starts_with(graph.dependencies.scope(source)) {
                    report.error(Some(id), "kind", "iteration_scope", format!("'{source}' is inside a different loop scope; accumulate its result in an outer variable"));
                }
                if !outgoing[from].iter().any(|(_, target)| *target == to) {
                    outgoing[from].push(("", to));
                    incoming[to] += 1;
                }
            }
        }
        // A user graph can be a thousand nodes deep; keep traversal off the call stack.
        // Include data edges: a forward control graph can still deadlock on circular inputs.
        let mut degrees = incoming;
        let mut queue: Vec<_> = (0..n).filter(|i| degrees[*i] == 0).collect();
        let mut processed = 0;
        while let Some(i) = queue.pop() {
            processed += 1;
            for (_, to) in &outgoing[i] {
                let mut required = graph.available[i].clone();
                required.insert(i);
                graph.available[*to].extend(required);
                degrees[*to] -= 1;
                if degrees[*to] == 0 { queue.push(*to); }
            }
        }
        if processed != n {
            report.error(None, "edges", "cycle", "control/data dependencies contain a cycle");
            return graph;
        }
        for (id, sources) in &graph.dependencies.sources {
            for source in sources {
                let Some(&from) = graph.indices.get(source.as_str()) else { continue };
                for loop_id in graph.dependencies.scope(id) {
                    if source != loop_id && !graph.dependencies.scope(source).contains(loop_id)
                        && graph.available[from].contains(&graph.indices[loop_id.as_str()]) {
                        report.error(Some(id), "kind", "loop_dependency", format!("'{source}' waits for loop '{loop_id}' to finish and cannot supply its body"));
                    }
                }
            }
        }
        graph
    }
}
