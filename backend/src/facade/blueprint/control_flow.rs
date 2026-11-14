use super::{
    definition::{Literal, NodeKind, Workflow},
    dependencies::Dependencies,
    execution::{MAX_STATE_BYTES, MAX_VALUE_BYTES, Outcome, Values},
    runs::{Iteration, NodeStatus, RunState},
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) const MAX_EXECUTIONS: u32 = 100_000;
pub(super) const MAX_LOOP_ITERATIONS: u32 = 10_000;
pub(super) const MAX_LOOP_DEPTH: usize = 32;

pub(super) struct Visit {
    pub node_id: String,
    pub iteration: Vec<Iteration>,
}

enum Frame {
    Node(Visit),
    Repeat(Visit),
}

struct Cursor {
    iterations: u32,
    items: Option<Vec<Literal>>,
    bytes: usize,
}

pub(super) struct ControlFlow {
    pending: Vec<Frame>,
    cursors: HashMap<String, Cursor>,
    dependencies: Dependencies,
}

impl ControlFlow {
    pub fn new(workflow: &Workflow) -> Self {
        let pending = workflow
            .nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Entry))
            .map(|node| {
                vec![Frame::Node(Visit {
                    node_id: node.id.clone(),
                    iteration: vec![],
                })]
            })
            .unwrap_or_default();
        Self {
            pending,
            cursors: HashMap::new(),
            dependencies: Dependencies::new(workflow),
        }
    }

    pub fn next(&mut self, workflow: &Workflow, state: &mut RunState) -> Result<Option<Visit>, String> {
        loop {
            let skipped: Vec<_> = self.dependencies.sources.iter().filter(|(id, sources)| {
                state.nodes.get(*id).is_some_and(|node| node.status == NodeStatus::Pending)
                    && sources.iter().any(|source| state.nodes.get(source).is_some_and(|node| node.status == NodeStatus::Skipped))
            }).map(|(id, _)| id.as_str()).collect();
            if skipped.is_empty() { break; }
            Self::skip(workflow, state, skipped.into_iter());
        }
        self.pending.retain(|frame| state.nodes[&Self::visit(frame).node_id].status != NodeStatus::Skipped);
        for index in (0..self.pending.len()).rev() {
            let frame = &self.pending[index];
            let visit = Self::visit(frame);
            if matches!(frame, Frame::Repeat(_)) {
                let mut body = visit.iteration.clone();
                body.push(Iteration { node_id: visit.node_id.clone(), index: self.cursors[&visit.node_id].iterations - 1 });
                // A parked body node can wait for a later outer branch. Keep that iteration alive.
                if self.pending.iter().enumerate().any(|(other, frame)| other != index && Self::visit(frame).iteration.starts_with(&body))
                    || self.dependencies.scopes.iter().any(|(id, scope)| scope.contains(&visit.node_id) && matches!(state.nodes[id].status, NodeStatus::Pending | NodeStatus::Running)) {
                    continue;
                }
            }
            if !self.dependencies.sources[&visit.node_id].iter().all(|source| {
                state.nodes.get(source).is_some_and(|node| {
                    let scope = self.dependencies.scope(source);
                    node.status == NodeStatus::Completed && visit.iteration.starts_with(&node.iteration)
                        && node.iteration.len() == scope.len()
                        && (!self.cursors.contains_key(source) || self.dependencies.scope(&visit.node_id).contains(source))
                })
            }) { continue; }
            let frame = self.pending.remove(index);
            let visit = match frame {
                Frame::Node(visit) => { self.cursors.remove(&visit.node_id); visit }
                Frame::Repeat(visit) => { Self::reset(state, &visit.node_id); visit }
            };
            return Ok(Some(visit));
        }
        if self.pending.is_empty() { return Ok(None); }
        Err("no node can proceed: required inputs or an unfinished loop are still pending".into())
    }

    fn visit(frame: &Frame) -> &Visit {
        match frame { Frame::Node(visit) | Frame::Repeat(visit) => visit }
    }

    fn skip<'a>(workflow: &Workflow, state: &mut RunState, roots: impl Iterator<Item = &'a str>) {
        for id in Self::descendants(workflow, roots) {
            if let Some(node) = state.nodes.get_mut(&id) {
                if node.status == NodeStatus::Pending {
                    node.status = NodeStatus::Skipped;
                    state.skipped_executions += 1;
                }
            }
        }
    }

    pub fn loop_outcome(
        &mut self,
        id: &str,
        kind: &NodeKind,
        state: &mut RunState,
    ) -> Result<Outcome, String> {
        if !self.cursors.contains_key(id) {
            let (items, bytes) = if let NodeKind::ForEach { items, .. } = kind {
                let mut values = Values {
                    state,
                    remaining: MAX_VALUE_BYTES,
                };
                let Literal::Array(items) = values.resolve(items, 0)? else {
                    return Err("for_each requires a list".into());
                };
                let bytes = serde_json::to_vec(&items)
                    .map_err(|_| "could not serialize loop items")?
                    .len();
                if bytes > MAX_VALUE_BYTES { return Err("loop items exceed 1 MiB".into()); }
                if self
                    .cursors
                    .values()
                    .map(|cursor| cursor.bytes)
                    .sum::<usize>()
                    + bytes
                    > MAX_STATE_BYTES
                {
                    return Err("active loop snapshots exceed 4 MiB".into());
                }
                (Some(items), bytes)
            } else {
                (None, 0)
            };
            self.cursors.insert(
                id.into(),
                Cursor {
                    iterations: 0,
                    items,
                    bytes,
                },
            );
        }
        let cursor = self.cursors.get_mut(id).unwrap();
        let (enter, limit) = match kind {
            NodeKind::While {
                left,
                operator,
                right,
                max_iterations,
            } => {
                let mut values = Values {
                    state,
                    remaining: MAX_VALUE_BYTES,
                };
                (
                    operator.evaluate(&values.resolve(left, 0)?, &values.resolve(right, 0)?)?,
                    *max_iterations,
                )
            }
            NodeKind::ForEach { max_iterations, .. } => (
                cursor.iterations < cursor.items.as_ref().unwrap().len() as u32,
                *max_iterations,
            ),
            _ => return Err("expected a loop node".into()),
        };
        let mut outputs = BTreeMap::new();
        if enter {
            if cursor.iterations >= limit {
                return Err("loop iteration limit reached".into());
            }
            outputs.insert("index".into(), Literal::Integer(cursor.iterations.into()));
            if let Some(items) = &cursor.items {
                outputs.insert("item".into(), items[cursor.iterations as usize].clone());
            }
            cursor.iterations += 1;
        }
        outputs.insert(
            "iterations".into(),
            Literal::Integer(cursor.iterations.into()),
        );
        state
            .loops
            .get_mut(id)
            .ok_or("loop state is missing")?
            .iterations = cursor.iterations;
        Ok(Outcome {
            control_output: Some(if enter { "body" } else { "out" }.into()),
            outputs,
            assignment: None,
        })
    }

    pub fn advance(
        &mut self,
        workflow: &Workflow,
        state: &mut RunState,
        visit: Visit,
        selected: Option<&str>,
    ) -> Result<(), String> {
        let is_loop = state.loops.contains_key(&visit.node_id);
        if is_loop && selected == Some("body") {
            if visit.iteration.len() >= MAX_LOOP_DEPTH {
                return Err("loop nesting exceeds 32 levels".into());
            }
            let roots = workflow
                .edges
                .iter()
                .filter(|edge| edge.from == visit.node_id && edge.port == "body")
                .map(|edge| edge.to.as_str());
            for id in Self::descendants(workflow, roots) {
                Self::reset(state, &id);
                if let Some(progress) = state.loops.get_mut(&id) {
                    progress.iterations = 0;
                }
            }
            let mut iteration = visit.iteration.clone();
            iteration.push(Iteration {
                node_id: visit.node_id.clone(),
                index: state.loops[&visit.node_id].iterations - 1,
            });
            self.pending.push(Frame::Repeat(Visit {
                node_id: visit.node_id.clone(),
                iteration: visit.iteration,
            }));
            self.push_edges(workflow, &visit.node_id, "body", &iteration);
        } else {
            if is_loop {
                self.cursors.remove(&visit.node_id);
            }
            for edge in workflow
                .edges
                .iter()
                .filter(|edge| edge.from == visit.node_id && Some(edge.port.as_str()) != selected)
            {
                Self::skip(workflow, state, std::iter::once(edge.to.as_str()));
            }
            if let Some(selected) = selected {
                self.push_edges(workflow, &visit.node_id, selected, &visit.iteration);
            }
        }
        Ok(())
    }

    fn push_edges(&mut self, workflow: &Workflow, from: &str, port: &str, iteration: &[Iteration]) {
        for edge in workflow
            .edges
            .iter()
            .rev()
            .filter(|edge| edge.from == from && edge.port == port)
        {
            self.pending.push(Frame::Node(Visit {
                node_id: edge.to.clone(),
                iteration: iteration.to_vec(),
            }));
        }
    }

    fn reset(state: &mut RunState, id: &str) {
        if let Some(node) = state.nodes.get_mut(id) {
            node.status = NodeStatus::Pending;
            node.outputs.clear();
            node.error = None;
            node.chat_id = None;
            node.execution_id = None;
            node.iteration.clear();
        }
    }

    pub fn descendants<'a>(
        workflow: &Workflow,
        roots: impl Iterator<Item = &'a str>,
    ) -> Vec<String> {
        let mut pending: Vec<_> = roots.map(str::to_owned).collect();
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            pending.extend(
                workflow
                    .edges
                    .iter()
                    .filter(|edge| edge.from == id)
                    .map(|edge| edge.to.clone()),
            );
        }
        seen.into_iter().collect()
    }
}
