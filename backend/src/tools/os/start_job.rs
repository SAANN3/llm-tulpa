use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use super::jobs::{read_output, tool_error, JobView};
use super::shell::{check_command_permission, effective_command};
use crate::services::process;
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams,
    ToolPermission, ToolSerializationError,
};
use crate::tools::storage::normalize;

pub struct StartJobTool;

/// How long `start_job` waits for the job to finish by itself when the model doesn't
/// say — long enough for a command that fails immediately (a missing binary, a port
/// already in use) to show its error in the same result.
const DEFAULT_WAIT_SECONDS: u32 = 3;
const MAX_WAIT_SECONDS: u32 = 30;

/// Lines of the log included in `start_job`'s own result.
const RESULT_TAIL_LINES: usize = 40;

#[derive(Deserialize, ToolParams)]
struct StartJobArgs {
    #[tool(description = "The shell command to run in the background (e.g. 'npm run dev', 'cargo build --release').")]
    command: String,
    #[tool(description = "Directory to run the command from — absolute or relative, ~ expands to home. Defaults to this backend's own working directory if omitted.")]
    workdir: Option<String>,
    #[tool(description = "How many seconds to wait for the job to finish on its own before returning, default 3, at most 30. It returns immediately either way once it has finished; a job that's still running after this keeps running.")]
    wait_seconds: Option<u32>,
}

#[derive(Serialize)]
struct StartJobOut {
    job: JobView,
    /// The end of the job's output so far.
    output: String,
    output_truncated: bool,
    /// Repeated inside the result itself, not just the tool description — a model reads
    /// what a call actually returned more reliably than a description it saw earlier.
    note: &'static str,
}

#[async_trait]
impl Tool for StartJobTool {
    fn function_name(&self) -> &str {
        "os.start_job"
    }

    fn description(&self) -> &str {
        "Starts a shell command as a background job and returns right away with a job id and the \
         path of its log — use this, not `&` inside os.execute_command, for anything that keeps \
         running or takes a long time: a dev server, a watcher, a big build or install. The job \
         keeps running on its own; you don't need to wait for it or poll it. When it finishes you \
         get a message in this chat saying how it ended, so it's fine to finish your turn instead \
         of waiting. Read what it has printed so far with os.job_output, see every job with \
         os.list_jobs, stop one with os.job_kill. If the command fails right away (a missing \
         program, a port already in use), that error is in this call's own result. Runs with the \
         same access, and needs the same per-command approval, as os.execute_command."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        StartJobArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::ShellCommands]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: StartJobArgs = serde_json::from_value(data)?;

        Ok(check_command_permission(
            &args.command,
            args.workdir.as_deref(),
            scope.shared.get(&SharedBucket::ShellCommands),
            false,
        ))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: StartJobArgs = serde_json::from_value(data)?;

        let command = process::shell_command(
            &effective_command(&args.command),
            args.workdir.as_deref().map(|dir| normalize(std::path::Path::new(dir))),
        );

        let started = ctx
            .job_store
            .start(ctx.chat_id, &args.command, args.workdir.as_deref(), command)
            .await
            .map_err(tool_error)?;

        let wait = Duration::from_secs(u64::from(args.wait_seconds.unwrap_or(DEFAULT_WAIT_SECONDS).min(MAX_WAIT_SECONDS)));
        let job = ctx
            .job_store
            .wait_until_finished(ctx.chat_id, started.id, wait)
            .await
            .map_err(tool_error)?;

        let (output, output_truncated) = read_output(&ctx.job_store, &job, RESULT_TAIL_LINES).await?;

        Ok(serde_json::to_value(StartJobOut {
            note: if job.status.is_finished() {
                "The job already finished — its outcome is above; nothing more to wait for."
            } else {
                "The job is still running and keeps running on its own. You'll get a message in \
                 this chat when it finishes; read its output so far any time with os.job_output."
            },
            job: JobView::from(&job),
            output,
            output_truncated,
        })?)
    }
}
