//! Telling the model about background jobs and sub-agents that finished: each one becomes a stored
//! `notice` message in the chat, claimed so that two callers can't report the same job twice.

use std::sync::Arc;

use super::prompts::{self, command_preview, job_notice_text, SubagentEnd};
use super::tool_calls::ToolCalls;
use super::NoticeOut;
use crate::services::chat_store::{ChatStore, MessageTimings, NewMessage};
use crate::services::error::ErrorService;
use crate::services::job_store::{JobKind, JobRecord, JobStatus, JobStore};

#[derive(Clone)]
pub(super) struct Notices {
    chat_store: Arc<ChatStore>,
    /// Where finished background jobs are looked up when a chat's next turn starts. Also reachable to
    /// tools through the `ToolContext`.
    job_store: Arc<JobStore>,
    tool_calls: ToolCalls,
    /// See `INLINED_RESULT_FRACTION` — the most of a sub-agent's result a notice carries.
    max_inlined_result_bytes: u64,
}

impl Notices {
    pub(super) fn new(
        chat_store: Arc<ChatStore>,
        job_store: Arc<JobStore>,
        tool_calls: ToolCalls,
        max_inlined_result_bytes: u64,
    ) -> Self {
        Self { chat_store, job_store, tool_calls, max_inlined_result_bytes }
    }

    /// Persists a `notice` for every background job (or sub-agent) that has finished since the model
    /// was last told about one, and returns them — without calling the model. A client shows them at
    /// once and then has the model respond with `continue_chat`; splitting it that way is what lets
    /// the notice appear the moment the job ends instead of after a model call that can take a
    /// while. Empty if there's nothing to report, which is what makes a stale hint
    /// (`ServerEvent::JobFinished` for a job an in-progress turn already reported) harmless — the
    /// decision is made here, not by the caller, because only here is claiming the finished jobs
    /// atomic. Also empty while tool calls are still waiting to run (mid-turn, or paused on a
    /// confirmation): a notice has to come after every tool result already in the chat, never in the
    /// middle of an unfinished batch, so it goes out with the `continue_chat` that follows once
    /// they have.
    pub(super) async fn flush_notices(&self, chat_id: i64) -> Result<Vec<NoticeOut>, ErrorService> {
        // Possible: also return empty while a model call is in flight (see TOOLS.md, "Known gap").
        if !self.tool_calls.pending_tool_calls(chat_id).await?.is_empty() {
            return Ok(vec![]);
        }

        self.flush_job_notices(chat_id).await
    }

    /// Turns every background job of `chat_id` that has finished but not been reported
    /// into a persisted `notice` message, oldest first, and returns them. Called at the
    /// one point in every turn where appending is always safe — after the newest
    /// message already stored, before the model's own reply — so a notice can never
    /// land between an assistant message's tool calls and their results. Persisted (not
    /// just added to the prompt) so the model keeps seeing it on later turns, the chat
    /// reads the same after a reload as it did live, and the reply that follows makes
    /// sense next to it. Each job is claimed before its notice is written, so
    /// concurrent callers can't report it twice; a job whose notice fails to save is
    /// handed back so the next turn tries again.
    pub(super) async fn flush_job_notices(&self, chat_id: i64) -> Result<Vec<NoticeOut>, ErrorService> {
        let jobs = self.job_store.claim_unnotified(chat_id).await?;

        let mut notices = Vec::with_capacity(jobs.len());
        for job in jobs {
            let content = match job.kind {
                JobKind::Process => job_notice_text(&job),
                JobKind::Agent { .. } => self.agent_job_notice_text(&job).await,
            };
            let stored = self
                .chat_store
                .new_message(NewMessage {
                    chat_id,
                    role: "notice".to_string(),
                    content,
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
                .await;

            match stored {
                Ok(message) => notices.push(NoticeOut { content: message.content, created_at: message.created_at }),
                Err(e) => {
                    if let Err(undo) = self.job_store.unclaim(job.id).await {
                        tracing::error!(job_id = job.id, "couldn't hand a job back after its notice failed to save: {undo}");
                    }
                    return Err(e.into());
                }
            }
        }

        Ok(notices)
    }

    /// The `notice` text for a finished sub-agent job. Unlike a command's, it carries the outcome
    /// itself: the result of a run that succeeded, or why it didn't get one.
    async fn agent_job_notice_text(&self, job: &JobRecord) -> String {
        let prompt = command_preview(&job.command);

        match (job.status, job.exit_code) {
            (JobStatus::Lost, _) => prompts::subagent_job_notice(job.id, &prompt, SubagentEnd::Lost),
            (JobStatus::Exited, code) => {
                let (text, cut) = match self.job_store.read_log_head(job, self.max_inlined_result_bytes).await {
                    Ok(read) => read,
                    Err(e) => (format!("(its result could not be read: {e})"), false),
                };
                let end = if code == Some(0) {
                    SubagentEnd::Finished { result: &text, cut }
                } else {
                    SubagentEnd::Failed { result: &text, cut }
                };
                prompts::subagent_job_notice(job.id, &prompt, end)
            }
            _ => prompts::subagent_job_notice(job.id, &prompt, SubagentEnd::Ended),
        }
    }
}
