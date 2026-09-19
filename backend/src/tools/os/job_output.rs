use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::jobs::{read_output, tool_error, JobView};
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct JobOutputTool;

const DEFAULT_TAIL_LINES: u32 = 100;

#[derive(Deserialize, ToolParams)]
struct JobOutputArgs {
    #[tool(description = "The id of a job in this chat, as returned by os.start_job or listed by os.list_jobs.")]
    job_id: i64,
    #[tool(description = "How many lines from the end of its output to return, default 100. The output is also capped at 40,000 characters.")]
    tail_lines: Option<u32>,
}

#[derive(Serialize)]
struct JobOutputOut {
    job: JobView,
    /// The end of the job's combined stdout and stderr.
    output: String,
    /// `true` when earlier output was cut off — only the end is ever returned.
    output_truncated: bool,
}

#[async_trait]
impl Tool for JobOutputTool {
    fn function_name(&self) -> &str {
        "os.job_output"
    }

    fn description(&self) -> &str {
        "Returns the current status of a background job (running, exited with its exit code, \
         killed, or lost if the backend restarted while it ran) together with the end of what it \
         has printed so far — stdout and stderr combined, in order. Works the same while the job \
         is still running and after it has finished, until the log of a long-finished job is cleaned \
         up (then this says so). Only jobs started in this chat."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        JobOutputArgs::tool_properties()
    }

    // No permission gating: this only reads the output of a command that was already
    // approved when it was started, and only for a job belonging to this very chat.
    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: JobOutputArgs = serde_json::from_value(data)?;

        let job = ctx.job_store.get(ctx.chat_id, args.job_id).await.map_err(tool_error)?;
        let tail_lines = args.tail_lines.unwrap_or(DEFAULT_TAIL_LINES) as usize;
        let (output, output_truncated) = read_output(&ctx.job_store, &job, tail_lines).await?;

        Ok(serde_json::to_value(JobOutputOut { job: JobView::from(&job), output, output_truncated })?)
    }
}
