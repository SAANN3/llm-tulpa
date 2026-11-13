use super::{
    definition::{Input, Literal},
    events::BlueprintEvent,
    execution::{MAX_STATE_BYTES, MAX_VALUE_BYTES, Outcome, Values},
    runs::{NodeStatus, PendingTool, RunDetail, RunStatus},
    validation,
};
use crate::{
    facade::agent::{Agent, Allowance},
    services::{
        blueprint_store::BlueprintStore,
        chat_store::{ChatStore, ToolCallOut},
        error::ErrorService,
        event_bus::{EventBus, ServerEvent},
        tools::ToolService,
    },
};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub(super) struct ToolNodes {
    store: Arc<BlueprintStore>,
    agent: Arc<Agent>,
    chats: Arc<ChatStore>,
    tools: Arc<ToolService>,
    events: Arc<EventBus>,
}
impl ToolNodes {
    pub fn new(
        store: Arc<BlueprintStore>,
        agent: Arc<Agent>,
        chats: Arc<ChatStore>,
        tools: Arc<ToolService>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            store,
            agent,
            chats,
            tools,
            events,
        }
    }
    pub async fn execute(
        &self,
        user_id: i64,
        run: &mut RunDetail,
        node_id: &str,
        name: &str,
        arguments: &BTreeMap<String, Input>,
        require_confirmation: bool,
        notify: &Notify,
        cancel: &CancellationToken,
    ) -> Result<Outcome, String> {
        let mut values = Values {
            state: &run.state,
            remaining: MAX_VALUE_BYTES,
        };
        let resolved = values.fields(arguments, 0)?;
        let tool = self
            .tools
            .get_tool(user_id, name)
            .await
            .ok_or_else(|| format!("tool '{name}' is unavailable"))?;
        let report = validation::tool_arguments(&resolved, &tool.required_properties());
        if !report.valid {
            return Err(report
                .diagnostics
                .into_iter()
                .map(|d| format!("{}: {}", d.field, d.message))
                .collect::<Vec<_>>()
                .join("; "));
        }
        let arguments =
            serde_json::to_value(resolved).map_err(|_| "could not serialize tool arguments")?;
        let chat_id = match run.chat_id {
            Some(id) => {
                self.chats.owned_chat(user_id, id).await.map_err(message)?;
                id
            }
            None => {
                let chat = self
                    .chats
                    .create_service_chat(
                        user_id,
                        format!("Blueprint {} / run {}", run.run.blueprint_id, run.run.id),
                    )
                    .await
                    .map_err(message)?;
                if !self
                    .store
                    .attach_run_chat(user_id, run.run.blueprint_id, run.run.id, chat.id)
                    .await
                    .map_err(message)?
                {
                    return Err("run was stopped".into());
                }
                run.chat_id = Some(chat.id);
                chat.id
            }
        };
        let mut scopes = Vec::new();
        if require_confirmation {
            loop {
                let view = self
                    .agent
                    .inspect_tool_call(chat_id, name.into(), arguments.clone(), &scopes)
                    .await
                    .map_err(message)?;
                let Some(grant) = view.offered_grant().cloned() else {
                    break;
                };
                let request_id = format!("{:032x}", rand::random::<u128>());
                run.state.pending_tool = Some(PendingTool {
                    request_id: request_id.clone(),
                    node_id: node_id.into(),
                    call: view,
                    answer: None,
                });
                run.state.nodes.get_mut(node_id).unwrap().status = NodeStatus::WaitingForPermission;
                if serde_json::to_vec(&run.state)
                    .map_err(|_| "could not serialize permission state")?
                    .len()
                    > MAX_STATE_BYTES
                {
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
                match allowance {
                    Allowance::Deny => return Err("tool call was denied".into()),
                    Allowance::Permanent => self
                        .agent
                        .allow_tool_scope(chat_id, name.into(), grant.scope)
                        .await
                        .map_err(message)?,
                    Allowance::OnlyNow => scopes.push(grant.scope),
                }
            }
        }
        if cancel.is_cancelled() {
            return Err("run was stopped".into());
        }
        // As in the ordinary agent, a tool already executing finishes before Stop is reported complete.
        let result = self
            .agent
            .call_tool(
                chat_id,
                ToolCallOut {
                    tool_name: name.into(),
                    arguments,
                },
                &scopes,
                !require_confirmation,
            )
            .await
            .map_err(message)?;
        if !result.success {
            return Err(result.err.unwrap_or_else(|| "tool call failed".into()));
        }
        if serde_json::to_vec(&result.content)
            .map_err(|_| "could not serialize tool result")?
            .len()
            > MAX_VALUE_BYTES
        {
            return Err("tool result exceeds 1 MiB".into());
        }
        let value: Literal = serde_json::from_value(result.content)
            .map_err(|_| "tool result cannot be represented as a workflow value")?;
        Ok(Outcome {
            port: Some("out".into()),
            outputs: BTreeMap::from([("result".into(), value)]),
        })
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
        .unwrap_or_else(|| "tool execution failed".into())
}
