use std::sync::{Arc, OnceLock, Weak};

use async_trait::async_trait;

use super::base::ToolError;
use super::llm::{return_agent::ReturnAgentTool, run_agent::RunAgentTool};
use super::os::{
    job_kill::JobKillTool, job_output::JobOutputTool, list_jobs::ListJobsTool, start_job::StartJobTool,
};

/// Background-job tools a sub-agent doesn't get. A run ends when the sub-agent stops calling
/// tools, and the last thing it wrote is taken as its answer — so a sub-agent that started a job
/// and then wrote "waiting for it to finish" would hand that sentence back as the result.
const JOB_TOOLS: [&str; 4] =
    [StartJobTool::NAME, JobOutputTool::NAME, ListJobsTool::NAME, JobKillTool::NAME];

/// Whether a chat may see and call the tool `name`. A sub-agent gets everything a normal chat
/// does except starting sub-agents of its own (delegation is one level deep) and the job tools;
/// a normal chat never gets `llm.return_agent`, which only means something inside a sub-agent.
pub fn available_to(name: &str, is_subagent: bool) -> bool {
    match name {
        RunAgentTool::NAME => !is_subagent,
        ReturnAgentTool::NAME => is_subagent,
        _ if JOB_TOOLS.contains(&name) => !is_subagent,
        _ => true,
    }
}

/// A sub-agent that has been started: the background job it runs as, and the chat it works in.
pub struct SubagentStarted {
    pub job_id: i64,
    pub chat_id: i64,
}

/// What a tool needs from the agent to start a sub-agent. A trait, not `Agent` itself, so the
/// tools layer never depends on the facade that runs it.
#[async_trait]
pub trait SubagentRunner: Send + Sync {
    /// Starts a sub-agent for `parent_chat_id` on `prompt` as a background job and returns at
    /// once. The result reaches the parent the way a finished command's does: a notice in the
    /// chat when the job ends.
    async fn start_subagent(&self, parent_chat_id: i64, prompt: String) -> Result<SubagentStarted, ToolError>;
}

/// Where a tool reaches the sub-agent runner through `ToolContext`. The runner is the `Agent`,
/// which itself holds the `ToolContext` — a direct reference would be a cycle in both
/// construction and ownership. So the handle starts empty, is bound once the `Agent` sits in its
/// `Arc`, and holds only a `Weak` so the agent's lifetime stays its owner's business.
pub struct SubagentHandle {
    runner: OnceLock<Weak<dyn SubagentRunner>>,
}

impl SubagentHandle {
    pub fn new() -> Self {
        Self { runner: OnceLock::new() }
    }

    /// Binds the runner. Only the first call takes effect.
    pub fn bind(&self, runner: Weak<dyn SubagentRunner>) {
        let _ = self.runner.set(runner);
    }

    pub async fn start(&self, parent_chat_id: i64, prompt: String) -> Result<SubagentStarted, ToolError> {
        let runner: Arc<dyn SubagentRunner> = self
            .runner
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| ToolError::FailedUnknown("sub-agents aren't available here".to_string()))?;
        runner.start_subagent(parent_chat_id, prompt).await
    }
}
