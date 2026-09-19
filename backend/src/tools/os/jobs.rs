//! What the `os` job tools (`start_job`, `job_output`, `job_kill`, `list_jobs`) share:
//! the one shape a job is reported back to the model in, and how a log is turned into
//! output.

use chrono::SecondsFormat;
use serde::Serialize;

use super::shell::tail_output;
use crate::services::job_store::{JobRecord, JobStore, JobStoreErrors};
use crate::tools::base::ToolError;

/// How much of the end of a log is read off disk at most. A few times
/// `MAX_OUTPUT_CHARS`, so cutting it down to lines and characters afterwards still
/// has plenty to work with, without ever loading a multi-gigabyte log into memory.
const LOG_READ_BYTES: u64 = 256 * 1024;

/// A job as the model sees it.
#[derive(Serialize)]
pub(super) struct JobView {
    job_id: i64,
    command: String,
    workdir: Option<String>,
    /// `running`, `exited`, `killed`, or `lost` (the backend restarted while it ran).
    status: &'static str,
    /// Only set once it has exited on its own.
    exit_code: Option<i32>,
    /// Where its combined stdout+stderr is being written — readable with
    /// `os.job_output`, or directly with `storage.read_file`. `null` once the log of a
    /// long-finished job has been cleaned up.
    log_path: Option<String>,
    started_at: String,
    finished_at: Option<String>,
}

impl From<&JobRecord> for JobView {
    fn from(job: &JobRecord) -> Self {
        Self {
            job_id: job.id,
            command: job.command.clone(),
            workdir: job.workdir.clone(),
            status: job.status.as_str(),
            exit_code: job.exit_code,
            log_path: job.log_available().then(|| job.log_path.to_string_lossy().to_string()),
            started_at: job.started_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            finished_at: job.finished_at.map(|at| at.to_rfc3339_opts(SecondsFormat::Secs, true)),
        }
    }
}

/// Turns a job store failure into what the model is shown.
pub(super) fn tool_error(e: JobStoreErrors) -> ToolError {
    ToolError::FailedUnknown(e.to_string())
}

/// The end of `job`'s log, at most `tail_lines` lines and `MAX_OUTPUT_CHARS`
/// characters, and whether anything was cut. Once a job is finished, reading its
/// output also counts as the model having seen how it ended — see
/// `JobStore::mark_notified`.
pub(super) async fn read_output(
    store: &JobStore,
    job: &JobRecord,
    tail_lines: usize,
) -> Result<(String, bool), ToolError> {
    let (text, cut_by_bytes) = store.read_log_tail(job, LOG_READ_BYTES).await.map_err(tool_error)?;

    let lines: Vec<&str> = text.lines().collect();
    let cut_by_lines = lines.len() > tail_lines;
    let text = lines[lines.len().saturating_sub(tail_lines)..].join("\n");

    let (text, cut_by_chars) = tail_output(text);

    if job.status.is_finished() {
        store.mark_notified(job.id).await.map_err(tool_error)?;
    }

    Ok((text, cut_by_bytes || cut_by_lines || cut_by_chars))
}
