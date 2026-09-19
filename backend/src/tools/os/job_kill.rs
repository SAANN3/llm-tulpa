use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::jobs::{tool_error, JobView};
use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct JobKillTool;

#[derive(Deserialize, ToolParams)]
struct JobKillArgs {
    #[tool(description = "The id of a running job in this chat, as returned by os.start_job or listed by os.list_jobs.")]
    job_id: i64,
}

#[derive(Serialize)]
struct JobKillOut {
    job: JobView,
    note: &'static str,
}

#[async_trait]
impl Tool for JobKillTool {
    fn function_name(&self) -> &str {
        "os.job_kill"
    }

    fn description(&self) -> &str {
        "Stops a running background job and everything it started. Fails if the job has already \
         finished, and says what state it's in. Only jobs started in this chat. Its output so \
         far stays readable with os.job_output."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        JobKillArgs::tool_properties()
    }

    // No permission gating: stopping a process this same chat started (with its own
    // approval) is never broader than starting it was.
    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: JobKillArgs = serde_json::from_value(data)?;

        let job = ctx.job_store.kill(ctx.chat_id, args.job_id).await.map_err(tool_error)?;

        Ok(serde_json::to_value(JobKillOut {
            job: JobView::from(&job),
            note: "Stopped. You won't get a message about this job ending — you already know.",
        })?)
    }
}
