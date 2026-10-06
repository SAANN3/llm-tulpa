mod entities;

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use entities::jobs;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, DbBackend, FromQueryResult, QueryOrder, Statement};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::task::AbortHandle;

use crate::services::error::ErrorService;
use crate::services::event_bus::{EventBus, ServerEvent};
use crate::services::process;

/// Owns every background job the model has started — the database row tracking each
/// one's state, the log file its output goes to, and the tokio task that watches the
/// process and records how it ended. Single responsibility, kept separate from the
/// `os.*_job` tools (which decide what may run and shape what the model sees) and from
/// `ChatStore` (which holds the `notice` message a finished job eventually turns into —
/// see `Agent`'s notice flushing). Same isolated-SeaORM-entity shape as the other
/// stores: `jobs` is private to this module, callers only ever see `JobRecord`.
pub struct JobStore {
    db: DatabaseConnection,
    jobs_dir: PathBuf,
    events: Arc<EventBus>,
    /// How to stop each running `agent` job: its task, by job id. A task removes its own entry when
    /// it ends; killing the job aborts the task instead.
    agent_tasks: Arc<Mutex<HashMap<i64, AbortHandle>>>,
}

/// Where a job is in its life. `Exited` means it stopped on its own (whatever its exit
/// code); `Killed` means it was stopped on request; `Lost` means the backend restarted
/// while it was running, so how it ended — or whether it's still going — is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Exited,
    Killed,
    Lost,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Exited => "exited",
            JobStatus::Killed => "killed",
            JobStatus::Lost => "lost",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "running" => JobStatus::Running,
            "exited" => JobStatus::Exited,
            "killed" => JobStatus::Killed,
            _ => JobStatus::Lost,
        }
    }

    pub fn is_finished(self) -> bool {
        self != JobStatus::Running
    }
}

/// What a job runs: a shell command, or a sub-agent working in a chat of its own. Both are
/// started from a chat, finish on their own or are killed, and report back the same way — the
/// difference is what "running" means (a process id vs. a task) and what the log holds (the
/// command's output vs. the result the sub-agent handed back).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Process,
    Agent { chat_id: i64 },
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Process => "process",
            JobKind::Agent { .. } => "agent",
        }
    }
}

/// How a sub-agent's run ended, as `start_agent` records it.
pub struct AgentJobEnd {
    /// Whether it reached an answer. Recorded as exit code `0`, otherwise `1`.
    pub succeeded: bool,
    /// The answer, or why there isn't one. Written to the job's log.
    pub text: String,
}

/// What a row actually is, decoupled from the private SeaORM `jobs::Model` — same
/// pattern as `FileStore`'s `FileRecord`.
#[derive(Clone, Debug)]
pub struct JobRecord {
    pub id: i64,
    pub kind: JobKind,
    /// The command line, or for a sub-agent the prompt it was given.
    pub command: String,
    pub workdir: Option<String>,
    pub log_path: PathBuf,
    pub status: JobStatus,
    /// `None` while running, and for a job that never got to report one (`Killed`,
    /// `Lost`). A process ended by a signal it didn't handle reports `-1`.
    pub exit_code: Option<i32>,
    pub started_at: DateTimeUtc,
    pub finished_at: Option<DateTimeUtc>,
}

impl JobRecord {
    /// Whether the log file still exists — `false` once the retention sweep has removed
    /// it (see `JobStore::new`), after which the job's output is gone for good.
    pub fn log_available(&self) -> bool {
        !self.log_path.as_os_str().is_empty()
    }
}

impl From<jobs::Model> for JobRecord {
    fn from(model: jobs::Model) -> Self {
        let kind = match (model.kind.as_str(), model.agent_chat_id) {
            ("agent", Some(chat_id)) => JobKind::Agent { chat_id },
            _ => JobKind::Process,
        };
        Self {
            id: model.id,
            kind,
            command: model.command,
            workdir: model.workdir,
            log_path: PathBuf::from(model.log_path),
            status: JobStatus::from_db(&model.status),
            exit_code: model.exit_code,
            started_at: model.started_at,
            finished_at: model.finished_at,
        }
    }
}

impl JobStore {
    /// Holds an already-connected, already-migrated connection (see `services::bootstrap`),
    /// and ensures `jobs_dir` exists on disk.
    /// Any row still marked `running` belongs to a previous backend process — nothing
    /// is watching it anymore — so it's marked `lost` here rather than left claiming
    /// to be running forever. The log of every finished job older than
    /// `log_retention_days` is deleted here too (`0` keeps them all): a log has to
    /// outlive the job by long enough for the model or the user to read it, so this is
    /// the one place they're cleaned up, not the moment a job ends.
    pub async fn new(
        db: DatabaseConnection,
        jobs_dir: PathBuf,
        log_retention_days: u64,
        events: Arc<EventBus>,
    ) -> Self {
        tokio::fs::create_dir_all(&jobs_dir)
            .await
            .unwrap_or_else(|e| panic!("failed to create jobs directory '{}': {e}", jobs_dir.display()));

        db.execute_unprepared("UPDATE jobs SET status = 'lost', finished_at = now() WHERE status = 'running'")
            .await
            .unwrap_or_else(|e| panic!("failed to mark leftover jobs as lost: {e}"));

        if log_retention_days > 0 {
            let removed = remove_old_logs(&db, log_retention_days).await;
            if removed > 0 {
                tracing::info!("removed {removed} background job log(s) older than {log_retention_days} day(s)");
            }
        }

        Self { db, jobs_dir, events, agent_tasks: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Starts `process` as a new background job for `chat_id` and returns as soon as it
    /// has been spawned. `command` is only what's recorded and shown back (the
    /// model's own command line); `child_command` is what actually runs, already built by the
    /// caller (shell, working directory, environment). Output goes to a log file, not a
    /// pipe — nothing here waits on it, and a process that keeps running keeps writing
    /// to it undisturbed. A task watches the process and, when it ends on its own,
    /// records the exit code and publishes `ServerEvent::JobFinished`.
    pub async fn start(
        &self,
        chat_id: i64,
        command: &str,
        workdir: Option<&str>,
        mut child_command: tokio::process::Command,
    ) -> Result<JobRecord, JobStoreErrors> {
        let inserted = jobs::ActiveModel {
            chat_id: Set(chat_id),
            command: Set(command.to_string()),
            workdir: Set(workdir.map(str::to_string)),
            status: Set(JobStatus::Running.as_str().to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        let id = inserted.id;

        let log_path = self.jobs_dir.join(format!("{id}.log"));
        let spawned = (|| -> std::io::Result<tokio::process::Child> {
            let stdout_log = std::fs::File::create(&log_path)?;
            let stderr_log = stdout_log.try_clone()?;
            child_command
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout_log))
                .stderr(Stdio::from(stderr_log));
            process::detach(&mut child_command);
            child_command.spawn()
        })();

        let child = match spawned {
            Ok(child) => child,
            Err(e) => {
                // Never started, so there's nothing for anyone to be told about later.
                self.execute(
                    "UPDATE jobs SET status = 'exited', exit_code = -1, finished_at = now(), notified = TRUE \
                     WHERE id = $1",
                    [id.into()],
                )
                .await?;
                return Err(JobStoreErrors::Io(format!("couldn't start the command: {e}")));
            }
        };

        let pid = child.id();
        self.execute(
            "UPDATE jobs SET log_path = $2, pid = $3 WHERE id = $1",
            [
                id.into(),
                log_path.to_string_lossy().to_string().into(),
                pid.map(i64::from).into(),
            ],
        )
        .await?;

        let db = self.db.clone();
        let events = self.events.clone();
        tokio::spawn(async move {
            let mut child = child;
            let exit_code = match child.wait().await {
                Ok(status) => status.code().unwrap_or(-1),
                Err(_) => -1,
            };
            record_exit(&db, &events, id, exit_code).await;
        });

        self.get(chat_id, id).await
    }

    /// Starts `run` — a sub-agent's whole run — as a background job of `chat_id`, and returns as soon
    /// as the task is going. `prompt` is what's recorded and shown back; `agent_chat_id` is the chat
    /// the sub-agent works in. When `run` ends, its text is written to the job's log and the job is
    /// recorded as ended, which publishes `ServerEvent::JobFinished` — from there it reports back
    /// exactly as a finished command does. The log file exists (empty) from the start, so reading
    /// the output of a job that's still running, or was killed, works like it does for a command.
    pub async fn start_agent<F>(
        &self,
        chat_id: i64,
        prompt: &str,
        agent_chat_id: i64,
        run: F,
    ) -> Result<JobRecord, JobStoreErrors>
    where
        F: Future<Output = AgentJobEnd> + Send + 'static,
    {
        let inserted = jobs::ActiveModel {
            chat_id: Set(chat_id),
            command: Set(prompt.to_string()),
            status: Set(JobStatus::Running.as_str().to_string()),
            kind: Set(JobKind::Agent { chat_id: agent_chat_id }.as_str().to_string()),
            agent_chat_id: Set(Some(agent_chat_id)),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        let id = inserted.id;

        let log_path = self.jobs_dir.join(format!("{id}.log"));
        if let Err(e) = tokio::fs::File::create(&log_path).await {
            // Never started, so there's nothing for anyone to be told about later.
            self.execute(
                "UPDATE jobs SET status = 'exited', exit_code = -1, finished_at = now(), notified = TRUE \
                 WHERE id = $1",
                [id.into()],
            )
            .await?;
            return Err(JobStoreErrors::Io(format!("couldn't create the sub-agent's log: {e}")));
        }
        self.execute(
            "UPDATE jobs SET log_path = $2 WHERE id = $1",
            [id.into(), log_path.to_string_lossy().to_string().into()],
        )
        .await?;

        let db = self.db.clone();
        let events = self.events.clone();
        let tasks = self.agent_tasks.clone();
        // The task waits for its abort handle to be registered before it starts, so a job that ends
        // at once can't finish (and try to unregister) before there is anything to unregister.
        let (registered, ready) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = ready.await;
            let end = run.await;
            // The result goes into the log before the job is recorded as ended: the moment the
            // row says so, a notice may read it.
            if let Err(e) = tokio::fs::write(&log_path, &end.text).await {
                tracing::error!("couldn't write sub-agent job {id}'s result to its log: {e}");
            }
            record_exit(&db, &events, id, if end.succeeded { 0 } else { 1 }).await;
            tasks.lock().unwrap().remove(&id);
        });
        self.agent_tasks.lock().unwrap().insert(id, handle.abort_handle());
        let _ = registered.send(());

        self.get(chat_id, id).await
    }

    /// One of `chat_id`'s jobs. A job that exists but belongs to a different chat is
    /// reported as `NotFound`, same as a job that doesn't exist — a chat never learns
    /// another chat's jobs are there.
    pub async fn get(&self, chat_id: i64, id: i64) -> Result<JobRecord, JobStoreErrors> {
        let model = jobs::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .filter(|model| model.chat_id == chat_id)
            .ok_or(JobStoreErrors::NotFound)?;

        Ok(model.into())
    }

    /// Every job `chat_id` has started, oldest first.
    pub async fn list_by_chat(&self, chat_id: i64) -> Result<Vec<JobRecord>, JobStoreErrors> {
        let models = jobs::Entity::find()
            .filter(jobs::Column::ChatId.eq(chat_id))
            .order_by_asc(jobs::Column::Id)
            .all(&self.db)
            .await?;

        Ok(models.into_iter().map(JobRecord::from).collect())
    }

    /// Every job the given chats started at or after `since`, oldest first — for usage stats, which
    /// scope by user through the chat ids (a job belongs to a chat, not to a user).
    pub async fn started_since(&self, chat_ids: &[i64], since: DateTimeUtc) -> Result<Vec<JobRecord>, JobStoreErrors> {
        let models = jobs::Entity::find()
            .filter(jobs::Column::ChatId.is_in(chat_ids.iter().copied()))
            .filter(jobs::Column::StartedAt.gte(since))
            .order_by_asc(jobs::Column::Id)
            .all(&self.db)
            .await?;

        Ok(models.into_iter().map(JobRecord::from).collect())
    }

    /// Waits until the job is no longer running or `timeout` passes, whichever comes
    /// first, and returns its state at that point.
    pub async fn wait_until_finished(
        &self,
        chat_id: i64,
        id: i64,
        timeout: Duration,
    ) -> Result<JobRecord, JobStoreErrors> {
        let deadline = Instant::now() + timeout;
        loop {
            let job = self.get(chat_id, id).await?;
            if job.status.is_finished() || Instant::now() >= deadline {
                return Ok(job);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Stops a running job and everything it started. Marked `Killed` and already
    /// reported (`notified`) before the process is signalled — whoever asked knows, so
    /// the process ending afterwards must not turn into a notice about it. Errors with
    /// `NotRunning` (carrying the job's actual state) if it had already finished.
    pub async fn kill(&self, chat_id: i64, id: i64) -> Result<JobRecord, JobStoreErrors> {
        let claimed = self
            .returning(
                "UPDATE jobs SET status = 'killed', finished_at = now(), notified = TRUE \
                 WHERE id = $1 AND chat_id = $2 AND status = 'running' RETURNING *",
                [id.into(), chat_id.into()],
            )
            .await?
            .into_iter()
            .next();

        let Some(job) = claimed else {
            let current = self.get(chat_id, id).await?;
            return Err(JobStoreErrors::NotRunning { id: current.id, status: current.status });
        };

        if let Some(pid) = job.pid.and_then(|pid| u32::try_from(pid).ok()) {
            process::kill_process_tree(pid).await;
        }
        if let Some(task) = self.agent_tasks.lock().unwrap().remove(&id) {
            task.abort();
        }

        Ok(job.into())
    }

    /// Atomically takes every finished job of `chat_id` whose outcome the model hasn't
    /// been told about, marking each one as told — so two callers racing (an in-loop
    /// turn and a push-triggered one) can never both get the same job. Oldest finish
    /// first. A caller that then fails to actually deliver the notice hands the ids back
    /// via `unclaim`.
    pub async fn claim_unnotified(&self, chat_id: i64) -> Result<Vec<JobRecord>, JobStoreErrors> {
        let mut claimed = self
            .returning(
                "UPDATE jobs SET notified = TRUE \
                 WHERE chat_id = $1 AND notified = FALSE AND status IN ('exited', 'lost') RETURNING *",
                [chat_id.into()],
            )
            .await?;

        claimed.sort_by_key(|job| (job.finished_at, job.id));
        Ok(claimed.into_iter().map(JobRecord::from).collect())
    }

    /// Whether a finished job of the chat hasn't been reported yet, without claiming it.
    pub async fn has_unnotified(&self, chat_id: i64) -> Result<bool, JobStoreErrors> {
        let found = self
            .returning(
                "SELECT * FROM jobs WHERE chat_id = $1 AND notified = FALSE AND status IN ('exited', 'lost') LIMIT 1",
                [chat_id.into()],
            )
            .await?;
        Ok(!found.is_empty())
    }

    /// Undoes `claim_unnotified` for one job whose notice couldn't be delivered.
    pub async fn unclaim(&self, id: i64) -> Result<(), JobStoreErrors> {
        self.execute("UPDATE jobs SET notified = FALSE WHERE id = $1", [id.into()]).await
    }

    /// Records that the model has now seen how this job ended (a tool result showed it),
    /// so it doesn't also get a notice about it.
    pub async fn mark_notified(&self, id: i64) -> Result<(), JobStoreErrors> {
        self.execute("UPDATE jobs SET notified = TRUE WHERE id = $1", [id.into()]).await
    }

    /// The last `max_bytes` of the job's log, and whether anything earlier was cut off.
    pub async fn read_log_tail(&self, job: &JobRecord, max_bytes: u64) -> Result<(String, bool), JobStoreErrors> {
        let io_error = |e: std::io::Error| JobStoreErrors::Io(format!("couldn't read '{}': {e}", job.log_path.display()));

        if !job.log_available() {
            return Err(JobStoreErrors::LogRemoved);
        }
        let mut file = match tokio::fs::File::open(&job.log_path).await {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(JobStoreErrors::LogRemoved),
            Err(e) => return Err(io_error(e)),
        };
        let len = file.metadata().await.map_err(io_error)?.len();
        let truncated = len > max_bytes;
        if truncated {
            file.seek(SeekFrom::Start(len - max_bytes)).await.map_err(io_error)?;
        }

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await.map_err(io_error)?;
        Ok((String::from_utf8_lossy(&bytes).to_string(), truncated))
    }

    /// The first `max_bytes` of the job's log, and whether anything after that was cut off — for a
    /// sub-agent's result, where the start is what matters (`read_log_tail` is for command output,
    /// where the end is).
    pub async fn read_log_head(&self, job: &JobRecord, max_bytes: u64) -> Result<(String, bool), JobStoreErrors> {
        let io_error = |e: std::io::Error| JobStoreErrors::Io(format!("couldn't read '{}': {e}", job.log_path.display()));

        if !job.log_available() {
            return Err(JobStoreErrors::LogRemoved);
        }
        let file = match tokio::fs::File::open(&job.log_path).await {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(JobStoreErrors::LogRemoved),
            Err(e) => return Err(io_error(e)),
        };
        let len = file.metadata().await.map_err(io_error)?.len();

        let mut bytes = Vec::new();
        file.take(max_bytes).read_to_end(&mut bytes).await.map_err(io_error)?;
        Ok((String::from_utf8_lossy(&bytes).to_string(), len > max_bytes))
    }

    async fn execute<const N: usize>(&self, sql: &str, values: [Value; N]) -> Result<(), JobStoreErrors> {
        self.db
            .execute_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
            .await?;
        Ok(())
    }

    /// Runs an `UPDATE ... RETURNING *` and maps every returned row back into a model.
    async fn returning<const N: usize>(&self, sql: &str, values: [Value; N]) -> Result<Vec<jobs::Model>, JobStoreErrors> {
        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
            .await?;

        Ok(rows
            .iter()
            .map(|row| jobs::Model::from_query_result(row, ""))
            .collect::<Result<Vec<_>, _>>()?)
    }
}

/// Deletes the log file of every finished job that ended more than `days` days ago and
/// blanks its `log_path`, so the row records that the output is gone (and the next sweep
/// skips it). Returns how many logs were removed. Best-effort: a file that won't delete
/// is left for the next start rather than failing the boot.
async fn remove_old_logs(db: &DatabaseConnection, days: u64) -> usize {
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT * FROM jobs WHERE status <> 'running' AND log_path <> '' \
             AND finished_at < now() - make_interval(days => $1)",
            [i32::try_from(days).unwrap_or(i32::MAX).into()],
        ))
        .await;

    let rows = match rows {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("couldn't look up old background job logs to remove: {e}");
            return 0;
        }
    };

    let mut removed = 0;
    for row in rows {
        let Ok(job) = jobs::Model::from_query_result(&row, "") else { continue };

        match tokio::fs::remove_file(&job.log_path).await {
            Ok(()) => removed += 1,
            // Already gone (deleted by hand, say) — just record it.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!("couldn't remove job {}'s log '{}': {e}", job.id, job.log_path);
                continue;
            }
        }

        let _ = db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE jobs SET log_path = '' WHERE id = $1",
                [job.id.into()],
            ))
            .await;
    }

    removed
}

/// What the watcher task does once the process ends by itself. Only touches a job still
/// marked `running` — one already `killed` (or `lost` across a restart) keeps the status
/// it was given, and produces no event.
async fn record_exit(db: &DatabaseConnection, events: &EventBus, id: i64, exit_code: i32) {
    let result = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE jobs SET status = 'exited', exit_code = $2, finished_at = now() \
             WHERE id = $1 AND status = 'running' RETURNING *",
            [id.into(), exit_code.into()],
        ))
        .await;

    match result {
        Ok(Some(row)) => match jobs::Model::from_query_result(&row, "") {
            Ok(job) => events.publish(ServerEvent::JobFinished { chat_id: job.chat_id, job_id: job.id }),
            Err(e) => tracing::error!("job {id} exited but its row couldn't be read back: {e}"),
        },
        Ok(None) => {}
        Err(e) => tracing::error!("couldn't record how job {id} ended: {e}"),
    }
}

pub enum JobStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    /// The job exists but had already finished — carries its actual state.
    NotRunning { id: i64, status: JobStatus },
    /// The job's log was removed by the retention sweep (or by hand).
    LogRemoved,
    Io(String),
}

impl From<DbErr> for JobStoreErrors {
    fn from(err: DbErr) -> Self {
        JobStoreErrors::QueryFailed(err)
    }
}

/// What a tool shows the model when a job operation fails.
impl fmt::Display for JobStoreErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStoreErrors::QueryFailed(e) => {
                tracing::error!("job store query failed: {e}");
                write!(f, "database query failed")
            }
            JobStoreErrors::NotFound => write!(f, "no such job in this chat"),
            JobStoreErrors::NotRunning { id, status } => {
                write!(f, "job {id} is not running (status: {})", status.as_str())
            }
            JobStoreErrors::LogRemoved => write!(
                f,
                "this job's log has been cleaned up — logs of finished jobs are only kept for a while"
            ),
            JobStoreErrors::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<JobStoreErrors> for ErrorService {
    fn from(err: JobStoreErrors) -> Self {
        match err {
            JobStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "no such job"),
            other => {
                tracing::error!("job store failed: {other}");
                ErrorService::internal("background job operation failed")
            }
        }
    }
}
