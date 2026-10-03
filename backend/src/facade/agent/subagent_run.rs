//! Running a sub-agent: the loop the frontend drives for an ordinary chat (`chat`, run the
//! pending tools, `continue_chat`, repeat), run here in the backend for a chat nobody is watching.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::Value;

use super::{Agent, AgentToolPermission};
use crate::services::chat_store::{MessageTimings, NewMessage};
use crate::services::error::ErrorService;
use crate::services::job_store::AgentJobEnd;
use crate::services::llm::{ThinkChoice, ThinkingCapability};
use crate::tools::base::ToolError;
use crate::tools::llm::return_agent::ReturnAgentTool;
use crate::tools::subagent::{SubagentRunner, SubagentStarted};

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

/// A sub-agent's attempt to hand its result back that the tool refused (a missing `output`, say).
struct FailedReturn {
    /// What the tool said.
    error: String,
    /// What the sub-agent wrote alongside the call — usually the answer it meant to return.
    attempt: String,
}

/// What running the sub-agent's pending tool calls came to.
struct PendingRun {
    /// The result, when `llm.return_agent` succeeded.
    answer: Option<String>,
    /// The error of the last `llm.return_agent` call that failed, if any did.
    failed_return: Option<String>,
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
        let live = self.mark_subagent_live(sub.id);
        let job = self
            .job_store
            .start_agent(parent.id, &prompt, sub.id, async move {
                // Held for the whole run; dropped when it ends or the job is killed.
                let _live = live;
                agent.run_to_end(sub_chat_id, auto_confirm, task_prompt).await
            })
            .await?;

        Ok(SubagentStarted { job_id: job.id, chat_id: sub.id })
    }

    /// Marks a sub-agent's chat as live until the returned guard is dropped — the same guard type
    /// the tool-execution mark uses, over the set `is_running` also consults.
    fn mark_subagent_live(&self, chat_id: i64) -> super::RunningToolGuard {
        self.live_subagents.lock().unwrap().insert(chat_id);
        super::RunningToolGuard { running: self.live_subagents.clone(), chat_id }
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
        let think = self.subagent_think(sub_chat_id).await?;
        let mut reply = self.chat(sub_chat_id, prompt, vec![], vec![], think.clone()).await?;
        let mut model_calls = 1;
        let mut failed_return: Option<FailedReturn> = None;
        let mut reminded = false;
        loop {
            if !reply.can_use_tools {
                // Stopping after a refused `llm.return_agent` isn't an answer: the last message is
                // usually just "I will call it now". It gets one reminder, then what it wrote is
                // handed over as it stands.
                if let Some(failed) = &failed_return {
                    if reminded {
                        return Ok(SubagentResult::Incomplete(unreturned_result(failed, &reply.content)));
                    }
                    reminded = true;
                    self.remind_to_return(sub_chat_id, &failed.error).await?;
                    reply = self.continue_chat(sub_chat_id, think.clone()).await?;
                    model_calls += 1;
                    continue;
                }

                return Ok(match reply.content.trim() {
                    "" => SubagentResult::Incomplete("the sub-agent stopped without writing an answer".to_string()),
                    answer => SubagentResult::Answer(answer.to_string()),
                });
            }

            let run = self.run_pending_unattended(sub_chat_id, auto_confirm).await?;
            if let Some(answer) = run.answer {
                return Ok(SubagentResult::Answer(answer));
            }
            if let Some(error) = run.failed_return {
                failed_return = Some(FailedReturn { error, attempt: reply.content.clone() });
            }

            if model_calls >= MAX_MODEL_CALLS {
                let mut message = format!("the sub-agent used all {MAX_MODEL_CALLS} of its model calls without finishing");
                if !reply.content.trim().is_empty() {
                    message.push_str(&format!("; its last message was: {}", reply.content.trim()));
                }
                return Ok(SubagentResult::Incomplete(message));
            }

            reply = self.continue_chat(sub_chat_id, think.clone()).await?;
            model_calls += 1;
        }
    }

    /// Tells the sub-agent its `llm.return_agent` call was refused and it isn't done. Stored as a
    /// `notice` (sent to the model as a user turn, shown as a muted marker), the same way the
    /// continuation prompt after a cut-off thought is.
    async fn remind_to_return(&self, chat_id: i64, error: &str) -> Result<(), ErrorService> {
        self.chat_store
            .new_message(NewMessage {
                chat_id,
                role: "notice".to_string(),
                content: format!(
                    "[System note: {} failed ({error}). Your run is not finished until it succeeds — call it \
                     again with your result in the `output` argument.]",
                    ReturnAgentTool::NAME
                ),
                tool_name: None,
                thinking: None,
                thought_duration_ms: None,
                tool_success: None,
                tool_denied: false,
                tool_calls: vec![],
                images: vec![],
                file_ids: vec![],
                prompt_tokens: None,
                eval_tokens: None,
                timings: MessageTimings::default(),
            })
            .await?;
        Ok(())
    }

    /// The `think` setting a sub-agent's model calls use. Nobody picks one for a sub-agent, so it is
    /// the plain default (thinking on) — except for a model whose template has no thinking at all,
    /// which Ollama may refuse a `think: true` for; that one is told not to think. Not being able
    /// to tell (Ollama unreachable) is not a reason to fail the run here — the model call itself
    /// reports that.
    async fn subagent_think(&self, sub_chat_id: i64) -> Result<Option<ThinkChoice>, ErrorService> {
        let chat = self.chat_store.chat(sub_chat_id).await?;
        let provider = self.providers.get(&chat.provider)?;
        Ok(match provider.thinking_capability(&chat.model).await {
            Ok(ThinkingCapability::Unsupported) => Some(ThinkChoice::Enabled(false)),
            _ => None,
        })
    }

    /// Runs every tool call the sub-agent's last reply asked for, in order. Stops at a successful
    /// `llm.return_agent`, which is the answer — calls queued behind it never execute. With
    /// `auto_confirm`, whatever the calls' escalations offer is granted first (all of them up front,
    /// then the calls run — the same order the frontend uses, and the reason `allow_scope` merges
    /// deltas instead of overwriting).
    async fn run_pending_unattended(&self, chat_id: i64, auto_confirm: bool) -> Result<PendingRun, ErrorService> {
        if auto_confirm {
            for call in self.pending_tool_calls(chat_id).await? {
                let view = self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?;
                if let AgentToolPermission::Denied { escalation: Some(grant), .. } = view.permission {
                    self.allow_scope(chat_id, view.name, grant.scope).await?;
                }
            }
        }

        let mut failed_return = None;
        loop {
            let out = self.run_next_tool(chat_id, None, true).await?;
            if out.tool_name == ReturnAgentTool::NAME {
                if out.success {
                    let answer = match out.content {
                        Value::String(text) => text,
                        other => other.to_string(),
                    };
                    return Ok(PendingRun { answer: Some(answer), failed_return: None });
                }
                failed_return = Some(out.err.unwrap_or_else(|| "the call was refused".to_string()));
            }
            if out.tools.is_empty() {
                return Ok(PendingRun { answer: None, failed_return });
            }
        }
    }
}

/// What a sub-agent that never managed to call `llm.return_agent` successfully hands back: the
/// message it wrote when it tried (usually the answer itself) and its last message, since neither
/// is reliably the answer on its own.
fn unreturned_result(failed: &FailedReturn, last_message: &str) -> String {
    let mut text = format!(
        "the sub-agent never managed to hand its result back — {} kept failing ({}).",
        ReturnAgentTool::NAME,
        failed.error
    );
    let (attempt, last) = (failed.attempt.trim(), last_message.trim());
    if !attempt.is_empty() {
        text.push_str(&format!("\nWhat it wrote when it tried:\n{attempt}"));
    }
    if !last.is_empty() && last != attempt {
        text.push_str(&format!("\nIts last message:\n{last}"));
    }
    text
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
