use super::{
    definition::{Literal, ModelChoice, NodeKind},
    execution::{MAX_VALUE_BYTES, Outcome, Values},
    permissions::Permissions,
    runs::RunDetail,
};
use crate::{
    facade::{
        agent::{Agent, Decision, RunPolicy},
        launch::LaunchFacade,
    },
    services::{
        blueprint_store::BlueprintStore,
        chat_store::{AgentPolicy, ChatStore},
        error::ErrorService,
        event_bus::RunEndReason,
        llm::ThinkChoice,
        tool_selection_store::ToolSelectionStore,
    },
};
use axum::http::StatusCode;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub(crate) struct AgentNodes {
    store: Arc<BlueprintStore>,
    agent: Arc<Agent>,
    chats: Arc<ChatStore>,
    selection: Arc<ToolSelectionStore>,
    launch: Arc<LaunchFacade>,
}
impl AgentNodes {
    pub fn new(
        store: Arc<BlueprintStore>,
        agent: Arc<Agent>,
        chats: Arc<ChatStore>,
        selection: Arc<ToolSelectionStore>,
        launch: Arc<LaunchFacade>,
    ) -> Self {
        Self {
            store,
            agent,
            chats,
            selection,
            launch,
        }
    }
    pub(super) async fn execute(
        &self,
        user_id: i64,
        run: &mut RunDetail,
        node_id: &str,
        kind: &NodeKind,
        permissions: &Permissions,
        notify: &Notify,
        cancel: &CancellationToken,
    ) -> Result<Outcome, String> {
        let NodeKind::Agent {
            model,
            prompt,
            system_prompt,
            tools,
            think,
            reuse_chat,
            write_notes,
            write_memory,
        } = kind
        else {
            return Err("expected an Agent node".into());
        };
        let mut values = Values {
            state: &run.state,
            remaining: MAX_VALUE_BYTES,
        };
        let prompt = string(values.resolve(prompt, 0)?)?;
        let system_prompt = system_prompt
            .as_ref()
            .map(|p| values.resolve(p, 0).and_then(string))
            .transpose()?;
        let (model_id, profile_id) = match model {
            ModelChoice::UserDefault => (None, None),
            ModelChoice::Model {
                id,
                launch_profile_id,
            } => (Some(*id), *launch_profile_id),
        };
        let (model, profile) = tokio::select! {
            _ = cancel.cancelled() => return Err("run was stopped".into()),
            result = self.launch.model_for_call(user_id, model_id, profile_id) => result.map_err(message)?,
        };
        let binding = if *reuse_chat {
            self.store
                .agent_chat(user_id, run.run.blueprint_id, node_id)
                .await
                .map_err(message)?
        } else {
            None
        };
        let chat = match binding {
            Some(id) => self.chats.owned_chat(user_id, id).await.map_err(message)?,
            None => self
                .chats
                .create_service_chat(
                    user_id,
                    format!("Blueprint {} / {}", run.run.blueprint_id, node_id),
                )
                .await
                .map_err(message)?,
        };
        if self.agent.has_run(chat.id) {
            return Err("agent chat is already running".into());
        }
        self.chats
            .configure_service_agent(
                user_id,
                chat.id,
                model.id,
                profile,
                tools.mode,
                AgentPolicy {
                    system_prompt,
                    write_notes: *write_notes,
                    write_memory: *write_memory,
                },
            )
            .await
            .map_err(message)?;
        self.selection
            .replace_chat(chat.id, &tools.names)
            .await
            .map_err(message)?;
        if !reuse_chat || binding.is_none() {
            self.store
                .bind_agent_chat(
                    user_id,
                    run.run.blueprint_id,
                    node_id,
                    reuse_chat.then_some(chat.id),
                )
                .await
                .map_err(message)?;
        }
        run.state.nodes.get_mut(node_id).unwrap().chat_id = Some(chat.id);
        if !self
            .store
            .save_progress(user_id, run.run.blueprint_id, run.run.id, &run.state)
            .await
            .map_err(message)?
        {
            return Err("run was stopped".into());
        }
        let result = self
            .drive(
                chat.id,
                prompt,
                think.clone(),
                user_id,
                run,
                node_id,
                permissions,
                notify,
                cancel,
            )
            .await;
        if result.is_err() {
            // A permission wait owns a provider claim too. Leave no pending call to replay on reuse.
            if let Err(error) = self.agent.stop(chat.id).await {
                if error.http_code != StatusCode::CONFLICT {
                    tracing::warn!(chat_id = chat.id, detail = ?error.message, "could not settle stopped agent calls");
                }
            }
        }
        result
    }
    async fn drive(
        &self,
        chat_id: i64,
        prompt: String,
        think: Option<ThinkChoice>,
        user_id: i64,
        run: &mut RunDetail,
        node_id: &str,
        permissions: &Permissions,
        notify: &Notify,
        cancel: &CancellationToken,
    ) -> Result<Outcome, String> {
        if cancel.is_cancelled() {
            return Err("run was stopped".into());
        }
        let mut handle = self
            .agent
            .start_turn(
                chat_id,
                prompt,
                vec![],
                vec![],
                think.clone(),
                RunPolicy::Service,
            )
            .await
            .map_err(message)?
            .run;
        loop {
            let end = tokio::select! {
                biased;
                result = handle.wait() => result.map_err(message)?,
                _ = cancel.cancelled() => {
                    self.agent.stop(chat_id).await.map_err(message)?;
                    handle.wait().await.map_err(message)?;
                    return Err("run was stopped".into());
                }
            };
            match end.reason {
                RunEndReason::Answered => {
                    let (messages, _) =
                        self.chats.messages(chat_id, 1, 0).await.map_err(message)?;
                    let text = messages
                        .into_iter()
                        .next()
                        .filter(|m| m.role == "assistant" && m.tool_calls.is_empty())
                        .ok_or("agent ended without a final message")?
                        .content;
                    if text.len() > MAX_VALUE_BYTES {
                        return Err("agent output exceeds 1 MiB".into());
                    }
                    return Ok(Outcome {
                        assignment: None,
                        control_output: Some("out".into()),
                        outputs: BTreeMap::from([("text".into(), Literal::String(text))]),
                    });
                }
                RunEndReason::WaitingForPermission => {
                    let calls = self
                        .agent
                        .turn_state(chat_id)
                        .await
                        .map_err(message)?
                        .pending;
                    let mut decisions = Vec::new();
                    for (index, call) in calls.into_iter().enumerate() {
                        if call.offered_grant().is_some() {
                            let allowance = permissions
                                .wait(user_id, run, node_id, call, notify, cancel)
                                .await?;
                            decisions.push(Decision { index, allowance });
                        }
                    }
                    if decisions.is_empty() {
                        return Err("agent is waiting without a grant to offer".into());
                    }
                    handle = self
                        .agent
                        .answer(chat_id, decisions, think.clone(), RunPolicy::Service)
                        .await
                        .map_err(message)?;
                }
                RunEndReason::Stopped => return Err("agent was stopped".into()),
                RunEndReason::Failed => {
                    return Err(end.detail.unwrap_or_else(|| "agent failed".into()));
                }
                RunEndReason::StepLimit => return Err("agent reached the user's step limit".into()),
            }
        }
    }
}
fn string(value: Literal) -> Result<String, String> {
    match value {
        Literal::String(text) => Ok(text),
        _ => Err("agent prompt must be a string".into()),
    }
}
fn message(error: impl Into<ErrorService>) -> String {
    error
        .into()
        .message
        .unwrap_or_else(|| "agent node failed".into())
}
