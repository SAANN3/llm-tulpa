use super::{
    definition::{Input, Literal},
    execution::{MAX_VALUE_BYTES, Outcome, Values},
    runs::RunDetail,
    permissions::Permissions,
    validation,
};
use crate::{
    facade::agent::{Agent, Allowance},
    services::{
        blueprint_store::BlueprintStore,
        chat_store::{ChatStore, ToolCallOut},
        error::ErrorService,
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
    permissions: Permissions,
}
impl ToolNodes {
    pub fn new(
        store: Arc<BlueprintStore>,
        agent: Arc<Agent>,
        chats: Arc<ChatStore>,
        tools: Arc<ToolService>,
        permissions: Permissions,
    ) -> Self {
        Self {
            store,
            agent,
            chats,
            tools,
            permissions,
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
                let allowance = self.permissions.wait(user_id, run, node_id, view, notify, cancel).await?;
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
            control_output: Some("out".into()),
            outputs: BTreeMap::from([("result".into(), value)]),
        })
    }
}
fn message(error: impl Into<ErrorService>) -> String {
    error
        .into()
        .message
        .unwrap_or_else(|| "tool execution failed".into())
}
