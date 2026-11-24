use super::{
    events::BlueprintEvent,
    runs::{NodeStatus, PendingTool, RunDetail, RunStatus},
};
use crate::{
    facade::agent::{AgentToolCall, Allowance},
    services::{
        blueprint_store::BlueprintStore,
        error::ErrorService,
        event_bus::{EventBus, ServerEvent},
    },
};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(super) struct Permissions {
    store: Arc<BlueprintStore>,
    events: Arc<EventBus>,
}
impl Permissions {
    pub fn new(store: Arc<BlueprintStore>, events: Arc<EventBus>) -> Self {
        Self { store, events }
    }
    pub async fn wait(
        &self,
        user_id: i64,
        run: &mut RunDetail,
        node_id: &str,
        call: AgentToolCall,
        notify: &Notify,
        cancel: &CancellationToken,
    ) -> Result<Allowance, String> {
        let request_id = format!("{:032x}", rand::random::<u128>());
        run.state.pending_tool = Some(PendingTool {
            request_id: request_id.clone(),
            node_id: node_id.into(),
            call,
            answer: None,
        });
        run.state.nodes.get_mut(node_id).unwrap().status = NodeStatus::WaitingForPermission;
        if !run.state.fits_budget().map_err(|_| "could not serialize permission state")? {
            run.state.pending_tool = None;
            return Err("run state exceeds 4 MiB".into());
        }
        if !self
            .store
            .wait_for_permission(user_id, run.run.blueprint_id, run.run.id, &run.state)
            .await
            .map_err(message)?
        {
            return Err("run was stopped".into());
        }
        run.run.status = RunStatus::WaitingForPermission;
        self.emit(user_id, run, node_id);
        let allowance = loop {
            let current = self
                .store
                .get_run(user_id, run.run.blueprint_id, run.run.id)
                .await
                .map_err(message)?;
            if let Some(answer) = current
                .state
                .pending_tool
                .as_ref()
                .filter(|p| p.request_id == request_id)
                .and_then(|p| p.answer)
            {
                break answer;
            }
            tokio::select! {
                _ = cancel.cancelled() => return Err("run was stopped".into()),
                _ = notify.notified() => {}
            }
        };
        run.state.pending_tool = None;
        run.state.nodes.get_mut(node_id).unwrap().status = NodeStatus::Running;
        if !self
            .store
            .resume_tool(user_id, run.run.blueprint_id, run.run.id, &run.state)
            .await
            .map_err(message)?
        {
            return Err("run was stopped".into());
        }
        run.run.status = RunStatus::Running;
        self.emit(user_id, run, node_id);
        Ok(allowance)
    }
    fn emit(&self, user_id: i64, run: &RunDetail, node_id: &str) {
        self.events.publish(ServerEvent::Blueprint {
            event: BlueprintEvent::run_changed(user_id, run, Some(node_id)),
        });
    }
}
fn message(error: impl Into<ErrorService>) -> String {
    error
        .into()
        .message
        .unwrap_or_else(|| "permission state failed".into())
}
