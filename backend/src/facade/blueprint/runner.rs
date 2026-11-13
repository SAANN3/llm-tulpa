use super::{
    definition::{NodeKind, Workflow},
    events::BlueprintEvent,
    execution,
    tool_nodes::ToolNodes,
    runs::{NodeStatus, RunDetail, RunState, RunStatus},
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
    ) -> Arc<Self> {
        Arc::new(Self {
            store,
            events,
            shutdown,
            tools,
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

    fn emit(&self, user_id: i64, run: &RunDetail, node_id: Option<&str>) {
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
            .save_progress(user_id, run.run.blueprint_id, run.run.id, &run.state)
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
        let graph_error = validation::graph_errors(&run.workflow)
            .diagnostics
            .into_iter()
            .find(|d| d.severity == Severity::Error)
            .map(|d| d.message)
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
        let mut pending = if status == RunStatus::Completed {
            run.workflow
                .nodes
                .iter()
                .find(|n| matches!(n.kind, NodeKind::Entry))
                .map(|n| vec![n.id.clone()])
                .unwrap_or_default()
        } else {
            vec![]
        };
        while let Some(node_id) = pending.pop() {
            if cancel.is_cancelled() {
                status = RunStatus::Cancelled;
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
            run.state.current_node = Some(node_id.clone());
            if !self.progress(user_id, &run, Some(&node_id)).await? {
                status = RunStatus::Cancelled;
                break;
            }
            let outcome = match &kind {
                NodeKind::Tool { name, arguments, require_confirmation } => self.tools.execute(user_id, &mut run, &node_id, name, arguments, *require_confirmation, notify, cancel).await,
                _ => execution::execute(&kind, &run.state, cancel).await,
            };
            if cancel.is_cancelled() {
                status = RunStatus::Cancelled;
                break;
            }
            match outcome {
                Ok(outcome) => {
                    let node = run.state.nodes.get_mut(&node_id).unwrap();
                    node.status = NodeStatus::Completed;
                    node.outputs = outcome.outputs;
                    if serde_json::to_vec(&run.state)?.len() > execution::MAX_STATE_BYTES {
                        let node = run.state.nodes.get_mut(&node_id).unwrap();
                        node.outputs.clear();
                        node.status = NodeStatus::Failed;
                        node.error = Some("run state exceeds 4 MiB".into());
                        run.state.error = node.error.clone();
                        status = RunStatus::Failed;
                        break;
                    }
                    run.state.current_node = None;
                    for edge in run.workflow.edges.iter().filter(|e| e.from == node_id) {
                        if outcome.port.as_deref() != Some(&edge.port) {
                            skip_branch(&run.workflow, &mut run.state, &edge.to);
                        }
                    }
                    for edge in
                        run.workflow.edges.iter().rev().filter(|e| {
                            e.from == node_id && outcome.port.as_deref() == Some(&e.port)
                        })
                    {
                        pending.push(edge.to.clone());
                    }
                    if !self.progress(user_id, &run, Some(&node_id)).await? {
                        status = RunStatus::Cancelled;
                        break;
                    }
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
fn skip_branch(workflow: &Workflow, state: &mut RunState, root: &str) {
    let mut pending = vec![root.to_owned()];
    while let Some(id) = pending.pop() {
        if let Some(node) = state.nodes.get_mut(&id) {
            if node.status != NodeStatus::Pending {
                continue;
            }
            node.status = NodeStatus::Skipped;
            pending.extend(
                workflow
                    .edges
                    .iter()
                    .filter(|e| e.from == id)
                    .map(|e| e.to.clone()),
            );
        }
    }
}
