//! Running a sub-agent: the loop the frontend drives for an ordinary chat (`chat`, run the
//! pending tools, `continue_chat`, repeat), run here in the backend for a chat nobody is watching.

// Nothing starts a sub-agent yet: `llm.run_agent` is the caller, and it lands after this.
#![allow(dead_code)]

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::Value;

use super::{Agent, AgentToolPermission};
use crate::services::error::ErrorService;
use crate::services::job_store::AgentJobEnd;
use crate::tools::base::ToolError;
use crate::tools::subagent::{SubagentRunner, SubagentStarted, RETURN_AGENT_TOOL};

/// Model calls a sub-agent gets before its run is cut off. A sub-agent that keeps calling tools
/// without ever answering would otherwise run for as long as the model keeps going, holding the
/// only GPU while the chat that delegated to it waits.
const MAX_MODEL_CALLS: usize = 100;

/// How much of the prompt goes into the sub-agent chat's name.
const NAME_PROMPT_CHARS: usize = 60;

/// How a sub-agent's run ended.
enum SubagentResult {
    /// The text it handed back with `llm.return_agent`, or, when it just stopped calling tools,
    /// its last message.
    Answer(String),
    /// It didn't reach an answer (it ran out of model calls, or stopped with nothing to say). The
    /// text says why and carries whatever it had written last, if anything.
    Incomplete(String),
}

impl Agent {
    /// Makes this agent the runner behind `ToolContext::subagents`. Called once, after the agent
    /// is in its `Arc` — see `SubagentHandle` for why this can't happen in `new`.
    pub fn bind_subagent_runner(self: &Arc<Self>) {
        let runner: Weak<Agent> = Arc::downgrade(self);
        self.tool_context.subagents.bind(runner);
    }

    /// Starts a sub-agent for `parent_chat_id` as a background job and returns as soon as it is
    /// going. It gets a new chat on the parent's model that begins with the parent's tool grants;
    /// the run itself (`run_to_end`) reports back through the job, like a finished command does.
    pub async fn start_subagent(&self, parent_chat_id: i64, prompt: String) -> Result<SubagentStarted, ErrorService> {
        let parent = self.chat_store.chat(parent_chat_id).await?;
        // The tool list already keeps a sub-agent from asking for this; this is the guard behind it.
        if parent.parent_chat_id.is_some() {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, "a sub-agent can't start another sub-agent"));
        }

        let auto_confirm = self.settings_store.auto_confirm(parent.user_id).await?;
        let sub = self.chat_store.create_subchat(&parent, sub_chat_name(&prompt)).await?;
        self.permission_store.copy_grants(parent.id, sub.id).await?;

        let agent = self.clone();
        let task_prompt = prompt.clone();
        let sub_chat_id = sub.id;
        let job = self
            .job_store
            .start_agent(parent.id, &prompt, sub.id, async move {
                agent.run_to_end(sub_chat_id, auto_confirm, task_prompt).await
            })
            .await?;

        Ok(SubagentStarted { job_id: job.id, chat_id: sub.id })
    }

    /// One sub-agent run, from waiting its turn to how it ended. A failure partway (Ollama
    /// unreachable, say) ends the run as unsuccessful with the reason, rather than vanishing.
    async fn run_to_end(&self, sub_chat_id: i64, auto_confirm: bool, prompt: String) -> AgentJobEnd {
        // Closed only if the semaphore were dropped, which nothing does.
        let _slot = self.subagent_slot.acquire().await;

        match self.drive_subagent(sub_chat_id, auto_confirm, prompt).await {
            Ok(SubagentResult::Answer(text)) => AgentJobEnd { succeeded: true, text },
            Ok(SubagentResult::Incomplete(text)) => AgentJobEnd { succeeded: false, text },
            Err(e) => AgentJobEnd {
                succeeded: false,
                text: format!("the sub-agent failed: {}", e.message.unwrap_or_else(|| "unknown error".to_string())),
            },
        }
    }

    /// Drives the sub-agent's chat with `prompt` until it hands back an answer, stops, or runs out
    /// of model calls. Nothing here can ask the user anything — a call the sub-agent isn't
    /// permitted is refused (see `run_next_tool`'s `unattended`), unless the user has auto-confirm
    /// on, in which case the sub-agent is granted what it asks for, as the frontend would do for an
    /// ordinary chat.
    async fn drive_subagent(
        &self,
        sub_chat_id: i64,
        auto_confirm: bool,
        prompt: String,
    ) -> Result<SubagentResult, ErrorService> {
        let mut reply = self.chat(sub_chat_id, prompt, vec![], vec![], None).await?;
        let mut model_calls = 1;
        loop {
            if !reply.can_use_tools {
                return Ok(match reply.content.trim() {
                    "" => SubagentResult::Incomplete("the sub-agent stopped without writing an answer".to_string()),
                    answer => SubagentResult::Answer(answer.to_string()),
                });
            }

            if let Some(answer) = self.run_pending_unattended(sub_chat_id, auto_confirm).await? {
                return Ok(SubagentResult::Answer(answer));
            }

            if model_calls >= MAX_MODEL_CALLS {
                let mut message = format!("the sub-agent used all {MAX_MODEL_CALLS} of its model calls without finishing");
                if !reply.content.trim().is_empty() {
                    message.push_str(&format!("; its last message was: {}", reply.content.trim()));
                }
                return Ok(SubagentResult::Incomplete(message));
            }

            reply = self.continue_chat(sub_chat_id, None).await?;
            model_calls += 1;
        }
    }

    /// Runs every tool call the sub-agent's last reply asked for, in order. Returns the answer
    /// when one of them was `llm.return_agent` — the run stops there, so calls queued behind it
    /// never execute. With `auto_confirm`, whatever the calls' escalations offer is granted first
    /// (all of them up front, then the calls run — the same order the frontend uses, and the reason
    /// `allow_scope` merges deltas instead of overwriting).
    async fn run_pending_unattended(&self, chat_id: i64, auto_confirm: bool) -> Result<Option<String>, ErrorService> {
        if auto_confirm {
            for call in self.pending_tool_calls(chat_id).await? {
                let view = self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?;
                if let AgentToolPermission::Denied { escalation: Some(grant), .. } = view.permission {
                    self.allow_scope(chat_id, view.name, grant.scope).await?;
                }
            }
        }

        loop {
            let out = self.run_next_tool(chat_id, None, true).await?;
            if out.tool_name == RETURN_AGENT_TOOL && out.success {
                return Ok(Some(match out.content {
                    Value::String(text) => text,
                    other => other.to_string(),
                }));
            }
            if out.tools.is_empty() {
                return Ok(None);
            }
        }
    }
}

/// The name of a sub-agent's chat: its prompt, flattened to one line and cut short.
fn sub_chat_name(prompt: &str) -> String {
    let flat = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = flat.chars().take(NAME_PROMPT_CHARS).collect();
    let ellipsis = if flat.chars().count() > NAME_PROMPT_CHARS { "..." } else { "" };
    format!("Sub-agent: {cut}{ellipsis}")
}

#[async_trait]
impl SubagentRunner for Agent {
    async fn start_subagent(&self, parent_chat_id: i64, prompt: String) -> Result<SubagentStarted, ToolError> {
        Agent::start_subagent(self, parent_chat_id, prompt)
            .await
            .map_err(|e| ToolError::FailedUnknown(e.message.unwrap_or_else(|| "the sub-agent failed".to_string())))
    }
}
