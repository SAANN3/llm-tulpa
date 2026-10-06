//! Running a sub-agent: starting it as a background job and running its chat on the turn runner with the
//! sub-agent policy (nobody can be asked for permission, and the run ends at `llm.return_agent`).

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use axum::http::StatusCode;

use super::runner::SubagentResult;
use super::Agent;
use crate::services::error::ErrorService;
use crate::services::job_store::AgentJobEnd;
use crate::services::llm::{ThinkChoice, ThinkingCapability};
use crate::tools::base::ToolError;
use crate::tools::subagent::{SubagentRunner, SubagentStarted};

/// How much of the prompt goes into the sub-agent chat's name.
const NAME_PROMPT_CHARS: usize = 60;

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
        self.tool_calls.copy_grants(parent.id, sub.id).await?;

        let agent = self.clone();
        let task_prompt = prompt.clone();
        let sub_chat_id = sub.id;
        let live = self.tool_calls.mark_live(sub.id);
        let job = self
            .tool_context
            .job_store
            .start_agent(parent.id, &prompt, sub.id, async move {
                // Held for the whole run; dropped when it ends or the job is killed.
                let _live = live;
                agent.run_to_end(sub_chat_id, auto_confirm, task_prompt).await
            })
            .await?;

        Ok(SubagentStarted { job_id: job.id, chat_id: sub.id })
    }

    /// One sub-agent run, from waiting its turn to how it ended. A failure partway (Ollama
    /// unreachable, say) ends the run as unsuccessful with the reason, rather than vanishing.
    ///
    /// Nothing in the run can ask the user anything — a call the sub-agent isn't permitted is refused,
    /// unless the user has auto-confirm on, in which case the sub-agent is granted what it asks for, as a
    /// user's own chat would be (see `Policy::Subagent`).
    async fn run_to_end(&self, sub_chat_id: i64, auto_confirm: bool, prompt: String) -> AgentJobEnd {
        // Closed only if the semaphore were dropped, which nothing does.
        let _slot = self.subagent_slot.acquire().await;

        let result = match self.subagent_think(sub_chat_id).await {
            Ok(think) => self.runner.run_subagent(sub_chat_id, prompt, think, auto_confirm).await,
            Err(e) => Err(e),
        };
        match result {
            Ok(SubagentResult::Answer(text)) => AgentJobEnd { succeeded: true, text },
            Ok(SubagentResult::Incomplete(text)) => AgentJobEnd { succeeded: false, text },
            Err(e) => AgentJobEnd {
                succeeded: false,
                text: format!("the sub-agent failed: {}", e.message.unwrap_or_else(|| "unknown error".to_string())),
            },
        }
    }

    /// The `think` setting a sub-agent's model calls use. Nobody picks one for a sub-agent, so it is
    /// the plain default (thinking on) — except for a model whose template has no thinking at all,
    /// which Ollama may refuse a `think: true` for; that one is told not to think. Not being able
    /// to tell (Ollama unreachable) is not a reason to fail the run here — the model call itself
    /// reports that.
    async fn subagent_think(&self, sub_chat_id: i64) -> Result<Option<ThinkChoice>, ErrorService> {
        let chat = self.chat_store.chat(sub_chat_id).await?;
        let provider = self.model.provider(&chat)?;
        Ok(match provider.thinking_capability(&chat.model).await {
            Ok(ThinkingCapability::Unsupported) => Some(ThinkChoice::Enabled(false)),
            _ => None,
        })
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
