use super::{
    definition::{Literal, NodeKind, Workflow},
    dependencies::Dependencies,
    execution::{MAX_STATE_BYTES, MAX_VALUE_BYTES, Outcome, Values},
    runs::{Iteration, NodeStatus, RunState},
    validation,
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) const MAX_EXECUTIONS: u32 = 100_000;
pub(super) const MAX_LOOP_ITERATIONS: u32 = 10_000;
pub(super) const MAX_REPEAT_DEPTH: usize = 32;

pub(super) struct Visit {
    pub node_id: String,
    pub iteration: Vec<Iteration>,
    pub input: Option<String>,
}

enum Frame {
    Node(Visit),
    Repeat(Visit),
    Merge(Visit),
    Close(Visit),
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
    active_merges: HashSet<String>,
    settled: HashSet<String>,
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
                    input: None,
                })]
            })
            .unwrap_or_default();
        Self {
            pending,
            cursors: HashMap::new(),
            dependencies: Dependencies::new(workflow),
            active_merges: HashSet::new(),
            settled: HashSet::new(),
        }
    }

    pub fn next(&mut self, workflow: &Workflow, state: &mut RunState) -> Result<Option<Visit>, String> {
        loop {
            self.settle(workflow, state);
            let mut closed = false;
            for index in (0..self.pending.len()).rev() {
                let frame = &self.pending[index];
                let visit = Self::visit(frame);
                if matches!(frame, Frame::Close(_)) {
                    if self.body_pending(state, index, &visit.node_id, &visit.iteration) { continue; }
                    let id = visit.node_id.clone();
                    self.pending.remove(index);
                    self.active_merges.remove(&id);
                    closed = true;
                    break;
                }
                if matches!(frame, Frame::Merge(_)) && self.active_merges.contains(&visit.node_id) { continue; }
                if matches!(frame, Frame::Repeat(_)) {
                    let mut body = visit.iteration.clone();
                    body.push(Iteration { node_id: visit.node_id.clone(), index: self.cursors[&visit.node_id].iterations - 1 });
                    // A parked body node can wait for a later outer branch. Keep that iteration alive.
                    if self.body_pending(state, index, &visit.node_id, &body) { continue; }
                }
                if !self.dependencies.required(&visit.node_id, visit.input.as_deref()).iter().all(|source| {
                    state.nodes.get(source).is_some_and(|node| {
                        let scope = self.dependencies.scope(source);
                        node.status == NodeStatus::Completed && visit.iteration.starts_with(&node.iteration)
                            && node.iteration.len() == scope.len()
                            && (!(self.cursors.contains_key(source) || self.active_merges.contains(source)) || self.dependencies.scope(&visit.node_id).contains(source))
                    })
                }) { continue; }
                let frame = self.pending.remove(index);
                let visit = match frame {
                    Frame::Node(visit) => { self.cursors.remove(&visit.node_id); visit }
                    Frame::Repeat(visit) => { Self::reset(state, &visit.node_id); visit }
                    Frame::Merge(visit) => {
                        let ids = Self::descendants(workflow, std::iter::once(visit.node_id.as_str()));
                        self.reset_region(workflow, state, &ids, Some(&visit.node_id));
                        visit
                    }
                    Frame::Close(_) => unreachable!(),
                };
                return Ok(Some(visit));
            }
            if closed { continue; }
            if self.pending.is_empty() { return Ok(None); }
            return Err("no node can proceed: required inputs or an unfinished repeat scope are still pending".into());
        }
    }

    fn body_pending(&self, state: &RunState, index: usize, id: &str, body: &[Iteration]) -> bool {
        self.pending.iter().enumerate().any(|(other, frame)| other != index && Self::visit(frame).iteration.starts_with(body))
            || self.dependencies.scopes.iter().any(|(node, scope)| scope.iter().any(|scope| scope == id) && matches!(state.nodes[node].status, NodeStatus::Pending | NodeStatus::Running))
    }

    fn visit(frame: &Frame) -> &Visit {
        match frame { Frame::Node(visit) | Frame::Repeat(visit) | Frame::Merge(visit) | Frame::Close(visit) => visit }
    }

    fn settle(&mut self, workflow: &Workflow, state: &mut RunState) {
        loop {
            let roots: Vec<_> = self.dependencies.sources.iter().filter(|(id, sources)| {
                !self.dependencies.merge_sources.contains_key(*id)
                    && state.nodes.get(*id).is_some_and(|node| node.status == NodeStatus::Pending)
                    && sources.iter().any(|source| state.nodes.get(source).is_some_and(|node| node.status == NodeStatus::Skipped))
            }).map(|(id, _)| id.clone()).collect();
            let mut changed = !roots.is_empty();
            self.skip_nodes(workflow, state, roots);
            let before = self.pending.len();
            self.pending.retain(|frame| {
                let visit = Self::visit(frame);
                if matches!(frame, Frame::Merge(_)) {
                    !self.dependencies.required(&visit.node_id, visit.input.as_deref()).iter()
                        .any(|source| state.nodes.get(source).is_some_and(|node| node.status == NodeStatus::Skipped))
                } else { state.nodes[&visit.node_id].status != NodeStatus::Skipped }
            });
            changed |= self.pending.len() != before;
            let skipped: Vec<_> = self.dependencies.merge_sources.keys().filter(|id| {
                state.nodes[*id].status == NodeStatus::Pending && !self.active_merges.contains(*id)
                    && !self.pending.iter().any(|frame| matches!(frame, Frame::Merge(visit) if &visit.node_id == *id))
                    && workflow.edges.iter().filter(|edge| &edge.to == *id).all(|edge| self.settled.contains(&edge.id))
            }).cloned().collect();
            changed |= !skipped.is_empty();
            self.skip_nodes(workflow, state, skipped);
            if !changed { break; }
        }
    }

    fn skip_nodes(&mut self, workflow: &Workflow, state: &mut RunState, mut roots: Vec<String>) {
        while let Some(id) = roots.pop() {
            if let Some(node) = state.nodes.get_mut(&id) {
                if node.status != NodeStatus::Pending { continue; }
                node.status = NodeStatus::Skipped;
                state.skipped_executions += 1;
                for edge in workflow.edges.iter().filter(|edge| edge.from == id) {
                    self.settled.insert(edge.id.clone());
                    // An inactive branch closes only its own Merge input, not the other deliveries.
                    if !self.dependencies.merge_sources.contains_key(&edge.to) { roots.push(edge.to.clone()); }
                }
            }
        }
    }

    fn skip_edge(&mut self, workflow: &Workflow, state: &mut RunState, edge_id: &str) {
        if let Some(edge) = workflow.edges.iter().find(|edge| edge.id == edge_id) {
            self.settled.insert(edge.id.clone());
            if !self.dependencies.merge_sources.contains_key(&edge.to) {
                self.skip_nodes(workflow, state, vec![edge.to.clone()]);
            }
        }
    }

    fn reset_region(&mut self, workflow: &Workflow, state: &mut RunState, ids: &[String], preserve_input: Option<&str>) {
        let members: HashSet<_> = ids.iter().map(String::as_str).collect();
        for id in ids {
            Self::reset(state, id);
            if let Some(progress) = state.loops.get_mut(id) { progress.iterations = 0; }
        }
        for edge in &workflow.edges {
            if members.contains(edge.to.as_str()) && Some(edge.to.as_str()) != preserve_input {
                self.settled.remove(&edge.id);
            }
        }
    }

    pub fn merge_outcome(&self, workflow: &Workflow, state: &RunState, visit: &Visit) -> Result<Outcome, String> {
        let node = workflow.nodes.iter().find(|node| node.id == visit.node_id).ok_or("missing Merge node")?;
        let NodeKind::Merge { inputs, value_type } = &node.kind else { return Err("expected Merge node".into()) };
        let input = visit.input.as_ref().and_then(|id| inputs.get(id)).ok_or("missing Merge input binding")?;
        let value = Values { state, remaining: MAX_VALUE_BYTES }.resolve(input, 0)?;
        validation::variable_value(&value, value_type)?;
        if serde_json::to_vec(&value).map_err(|_| "could not serialize Merge value")?.len() > MAX_VALUE_BYTES {
            return Err("Merge value exceeds 1 MiB".into());
        }
        Ok(Outcome { control_output: Some("out".into()), outputs: BTreeMap::from([("value".into(), value)]), assignment: None })
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
            if visit.iteration.len() >= MAX_REPEAT_DEPTH {
                return Err("repeat nesting exceeds 32 levels".into());
            }
            let roots = workflow
                .edges
                .iter()
                .filter(|edge| edge.from == visit.node_id && edge.port == "body")
                .map(|edge| edge.to.as_str());
            let ids = Self::descendants(workflow, roots);
            self.reset_region(workflow, state, &ids, None);
            let mut iteration = visit.iteration.clone();
            iteration.push(Iteration {
                node_id: visit.node_id.clone(),
                index: state.loops[&visit.node_id].iterations - 1,
            });
            self.pending.push(Frame::Repeat(Visit {
                node_id: visit.node_id.clone(),
                iteration: visit.iteration,
                input: None,
            }));
            self.push_edges(workflow, &visit.node_id, "body", &iteration);
        } else if self.dependencies.merge_sources.contains_key(&visit.node_id) {
            if visit.iteration.len() >= MAX_REPEAT_DEPTH { return Err("repeat nesting exceeds 32 levels".into()); }
            let mut body = visit.iteration;
            body.push(Iteration { node_id: visit.node_id.clone(), index: state.nodes[&visit.node_id].executions });
            self.active_merges.insert(visit.node_id.clone());
            self.pending.push(Frame::Close(Visit { node_id: visit.node_id.clone(), iteration: body.clone(), input: None }));
            self.push_edges(workflow, &visit.node_id, "out", &body);
        } else {
            if is_loop {
                self.cursors.remove(&visit.node_id);
            }
            for edge in workflow
                .edges
                .iter()
                .filter(|edge| edge.from == visit.node_id && Some(edge.port.as_str()) != selected)
            {
                self.skip_edge(workflow, state, &edge.id);
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
            self.settled.insert(edge.id.clone());
            let merge = self.dependencies.merge_sources.contains_key(&edge.to);
            let visit = Visit { node_id: edge.to.clone(), iteration: iteration.to_vec(), input: merge.then(|| edge.id.clone()) };
            self.pending.push(if merge { Frame::Merge(visit) } else { Frame::Node(visit) });
        }
    }

    fn reset(state: &mut RunState, id: &str) {
        if let Some(node) = state.nodes.get_mut(id) {
            // Only outputs of the current visit can feed data bindings.
            if node.status == NodeStatus::Completed {
                node.last_outputs = std::mem::take(&mut node.outputs);
            } else {
                node.outputs.clear();
            }
            node.status = NodeStatus::Pending;
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
