use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use super::jobs::{tool_error, JobView};
use crate::tools::base::{PropertyInfo, Tool, ToolContext, ToolError};

pub struct ListJobsTool;

#[derive(Serialize)]
struct ListJobsOut {
    jobs: Vec<JobView>,
}

#[async_trait]
impl Tool for ListJobsTool {
    fn function_name(&self) -> &str {
        "os.list_jobs"
    }

    fn description(&self) -> &str {
        "Lists every background job started in this chat, oldest first, with each one's id, \
         command, status (running, exited with its exit code, killed, or lost if the backend \
         restarted while it ran) and log path. Use it to find a job's id again, or to see what's \
         still running. Takes no arguments."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let records = ctx.job_store.list_by_chat(ctx.chat_id).await.map_err(tool_error)?;

        Ok(serde_json::to_value(ListJobsOut { jobs: records.iter().map(JobView::from).collect() })?)
    }
}
