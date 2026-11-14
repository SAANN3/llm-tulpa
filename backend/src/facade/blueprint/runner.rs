use super::{
    definition::NodeKind,
    variables::Variables,
    events::BlueprintEvent,
    execution,
    control_flow::{ControlFlow, MAX_EXECUTIONS},
    agent_nodes::AgentNodes,
    permissions::Permissions,
    plugin_nodes::PluginNodes,
    generation::Generation,
    tool_nodes::ToolNodes,
    runs::{NodeStatus, RunDetail, RunStatus},
    validation::{self, report::Severity},
};
use crate::facade::agent::Allowance;
use crate::services::{
    blueprint_store::{BlueprintStore, BlueprintStoreError},
    event_bus::{EventBus, ServerEvent},
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

pub(super) struct Runner {
    store: Arc<BlueprintStore>,
    events: Arc<EventBus>,
    shutdown: CancellationToken,
    tools: ToolNodes,
    generation: Generation,
    agents: AgentNodes,
    permissions: Permissions,
    plugins: PluginNodes,
    workers: Mutex<HashSet<i64>>,
    active: Mutex<HashMap<i64, ActiveRun>>,
}
struct ActiveRun {
    user_id: i64,
    cancel: CancellationToken,
    notify: Arc<Notify>,
}

impl Runner {
    pub fn new(
        store: Arc<BlueprintStore>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
        tools: ToolNodes,
        generation: Generation,
        agents: AgentNodes,
        permissions: Permissions,
        plugins: PluginNodes,
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            events,
            shutdown,
            tools,
            generation,
            agents,
            permissions,
            plugins,
            workers: Mutex::default(),
            active: Mutex::default(),
        })
    }
    pub async fn initialize(self: &Arc<Self>) -> Result<(), BlueprintStoreError> {
        for user_id in self.store.recover_runs().await? {
            self.wake(user_id).await;
        }
        Ok(())
    }
    pub async fn wake(self: &Arc<Self>, user_id: i64) {
        if self.shutdown.is_cancelled() {
            return;
        }
        if !self.workers.lock().await.insert(user_id) {
            return;
        }
        let this = self.clone();
        tokio::spawn(async move {
            this.drain(user_id).await;
        });
    }
    pub async fn stop(&self, user_id: i64) -> Result<Option<i64>, BlueprintStoreError> {
        let run = self.store.request_stop(user_id).await?;
        if let Some(run) = run {
            if let Some(active) = self.active.lock().await.get(&run.run.id) {
                active.cancel.cancel();
            }
            self.emit(user_id, &run, None);
            Ok(Some(run.run.id))
        } else {
            Ok(None)
        }
    }
    pub async fn answer_tool(&self, user_id: i64, id: i64, run_id: i64, request_id: &str, allowance: Allowance) -> Result<(), BlueprintStoreError> {
        self.store.answer_tool(user_id, id, run_id, request_id, allowance).await?;
        if let Some(active) = self.active.lock().await.get(&run_id) { active.notify.notify_one(); }
        Ok(())
    }
    pub async fn user_deleted(&self, user_id: i64) {
        for active in self
            .active
            .lock()
            .await
            .values()
            .filter(|r| r.user_id == user_id)
        {
            active.cancel.cancel();
        }
    }
    pub fn queue_changed(&self, user_id: i64) {
        self.events.publish(ServerEvent::Blueprint {
            event: BlueprintEvent::QueueChanged { user_id },
        });
    }

    pub(super) fn emit(&self, user_id: i64, run: &RunDetail, node_id: Option<&str>) {
        self.events.publish(ServerEvent::Blueprint {
            event: BlueprintEvent::run_changed(user_id, run, node_id),
        });
    }
    async fn drain(self: Arc<Self>, user_id: i64) {
        loop {
            // Enqueue's wake cannot be lost between observing an empty queue and removing its worker.
            // This lock is held only for claiming, never while executing or sleeping.
            let mut workers = self.workers.lock().await;
            if self.shutdown.is_cancelled() {
                workers.remove(&user_id);
                return;
            }
            let next = self.store.claim_run(user_id).await;
            let run = match next {
                Ok(Some(run)) => run,
                Ok(None) => {
                    workers.remove(&user_id);
                    return;
                }
                Err(error) => {
                    workers.remove(&user_id);
                    tracing::error!(user_id, ?error, "could not claim blueprint run");
                    return;
                }
            };
            drop(workers);
            self.queue_changed(user_id);
            let run_id = run.run.id;
            let cancel = self.shutdown.child_token();
            let notify = Arc::new(Notify::new());
            self.active.lock().await.insert(
                run_id,
                ActiveRun {
                    user_id,
                    cancel: cancel.clone(),
                    notify: notify.clone(),
                },
            );
            // A stop before token registration is remembered in the row, checked by save_progress.
            if let Err(error) = self.run(user_id, run, &notify, &cancel).await {
                tracing::error!(
                    user_id,
                    run_id,
                    ?error,
                    "could not persist blueprint execution; stopping queue worker"
                );
                self.active.lock().await.remove(&run_id);
                self.workers.lock().await.remove(&user_id);
                return;
            }
            self.active.lock().await.remove(&run_id);
        }
    }
    async fn progress(
        &self,
        user_id: i64,
        run: &RunDetail,
        node: Option<&str>,
    ) -> Result<bool, BlueprintStoreError> {
        let saved = self
            .store
            .save_progress(user_id, run.run.blueprint_id, run.run.id, &run.state, node.and_then(|id| Variables::changed(&run.workflow, &run.state, id)))
            .await?;
        if saved {
            self.emit(user_id, run, node);
        }
        Ok(saved)
    }
    async fn run(
        &self,
        user_id: i64,
        mut run: RunDetail,
        notify: &Notify,
        cancel: &CancellationToken,
    ) -> Result<(), BlueprintStoreError> {
        let graph_error = validation::graph_errors(&run.workflow, &self.plugins.catalog().await)
            .diagnostics
            .into_iter()
            .find(|d| d.severity == Severity::Error)
            .map(|d| d.message)
            .or_else(|| run.state.error.clone())
            .or_else(|| {
                run.workflow
                    .nodes
                    .iter()
                    .any(|n| !execution::supported(&n.kind))
                    .then(|| "snapshot contains a node without an executor".into())
            });
        let mut status = RunStatus::Completed;
        if let Some(error) = graph_error {
            run.state.error = Some(error);
            status = RunStatus::Failed;
        }
        let mut flow = ControlFlow::new(&run.workflow);
        while status == RunStatus::Completed {
            if cancel.is_cancelled() {
                status = RunStatus::Cancelled;
                break;
            }
            let visit = match flow.next(&run.workflow, &mut run.state) {
                Ok(Some(visit)) => visit,
                Ok(None) => break,
                Err(error) => {
                    run.state.error = Some(error);
                    status = RunStatus::Failed;
                    break;
                }
            };
            let node_id = visit.node_id.clone();
            if run.state.execution_count >= MAX_EXECUTIONS {
                run.state.error = Some("run execution limit reached".into());
                status = RunStatus::Failed;
                break;
            }
            let node = run
                .workflow
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .ok_or_else(|| {
                    BlueprintStoreError::Corrupt("run references missing node".into())
                })?;
            let kind = node.kind.clone();
            let progress = run
                .state
                .nodes
                .get_mut(&node_id)
                .ok_or_else(|| BlueprintStoreError::Corrupt("missing node state".into()))?;
            if progress.status != NodeStatus::Pending {
                run.state.error = Some("snapshot would execute a node more than once".into());
                status = RunStatus::Failed;
                break;
            }
            progress.status = NodeStatus::Running;
            run.state.execution_count += 1;
            progress.execution_id = Some(run.state.execution_count);
            progress.iteration = visit.iteration.clone();
            run.state.current_node = Some(node_id.clone());
            if !self.progress(user_id, &run, Some(&node_id)).await? {
                status = RunStatus::Cancelled;
                break;
            }
            let outcome = match &kind {
                NodeKind::Tool { name, arguments, require_confirmation } => self.tools.execute(user_id, &mut run, &node_id, name, arguments, *require_confirmation, notify, cancel).await,
                NodeKind::Generate { model, prompt, think } => self.generation.execute(user_id, &run.state, model, prompt, think.clone(), cancel).await,
                NodeKind::Plugin { .. } => self.plugins.execute(user_id, &run, &node_id, &kind, cancel).await,
                NodeKind::Agent { .. } => self.agents.execute(user_id, &mut run, &node_id, &kind, &self.permissions, notify, cancel).await,
                NodeKind::Merge { .. } => flow.merge_outcome(&run.workflow, &run.state, &visit),
                NodeKind::While { .. } | NodeKind::ForEach { .. } => flow.loop_outcome(&node_id, &kind, &mut run.state),
                _ => execution::execute(&node_id, &kind, &run.workflow, &run.state, cancel).await,
            };
            if cancel.is_cancelled() {
                status = RunStatus::Cancelled;
                break;
            }
            match outcome {
                Ok(outcome) => {
                    let previous = outcome.assignment.map(|(id, value)| {
                        let old = run.state.variables.insert(id.clone(), value);
                        (id, old)
                    });
                    let node = run.state.nodes.get_mut(&node_id).unwrap();
                    node.status = NodeStatus::Completed;
                    node.outputs = outcome.outputs;
                    if let Err(error) = flow.advance(&run.workflow, &mut run.state, visit, outcome.control_output.as_deref()) {
                        run.state.nodes.get_mut(&node_id).unwrap().status = NodeStatus::Failed;
                        run.state.nodes.get_mut(&node_id).unwrap().error = Some(error.clone());
                        run.state.error = Some(error);
                        status = RunStatus::Failed;
                        break;
                    }
                    if serde_json::to_vec(&run.state)?.len() > execution::MAX_STATE_BYTES {
                        if let Some((id, previous)) = previous {
                            match previous {
                                Some(value) => { run.state.variables.insert(id, value); }
                                None => { run.state.variables.remove(&id); }
                            }
                        }
                        let node = run.state.nodes.get_mut(&node_id).unwrap();
                        node.outputs.clear();
                        node.status = NodeStatus::Failed;
                        node.error = Some("run state exceeds 4 MiB".into());
                        run.state.error = node.error.clone();
                        status = RunStatus::Failed;
                        break;
                    }
                    run.state.nodes.get_mut(&node_id).unwrap().executions += 1;
                    run.state.current_node = None;
                    let error = match self.progress(user_id, &run, Some(&node_id)).await {
                        Ok(true) => continue,
                        Ok(false) if Variables::changed(&run.workflow, &run.state, &node_id).is_none() => {
                            status = RunStatus::Cancelled;
                            break;
                        }
                        Ok(false) => { status = RunStatus::Cancelled; None }
                        Err(BlueprintStoreError::VariableState(error)) => { status = RunStatus::Failed; Some(error) }
                        Err(error) => return Err(error),
                    };
                    // A rejected transaction must not leave an assignment applied only in the run snapshot.
                    if let Some((id, previous)) = previous {
                        match previous {
                            Some(value) => { run.state.variables.insert(id, value); }
                            None => { run.state.variables.remove(&id); }
                        }
                    }
                    let node = run.state.nodes.get_mut(&node_id).unwrap();
                    node.executions -= 1;
                    node.outputs.clear();
                    node.status = if error.is_some() { NodeStatus::Failed } else { NodeStatus::Cancelled };
                    node.error = error.clone();
                    run.state.error = error;
                    break;
                }
                Err(error) => {
                    let node = run.state.nodes.get_mut(&node_id).unwrap();
                    node.status = NodeStatus::Failed;
                    node.error = Some(error.clone());
                    run.state.error = Some(format!("node '{node_id}': {error}"));
                    status = RunStatus::Failed;
                    break;
                }
            }
        }
        if self.shutdown.is_cancelled() {
            status = RunStatus::Interrupted;
        }
        let changed = run.state.current_node.clone();
        let finished = self
            .store
            .finish_run(user_id, run.run.blueprint_id, run.run.id, status, run.state)
            .await?;
        self.emit(user_id, &finished, changed.as_deref());
        self.queue_changed(user_id);
        Ok(())
    }
}
