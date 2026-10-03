use std::sync::Arc;

use chrono::NaiveDate;

use crate::services::llama_runtime::LlamaRuntime;
use crate::services::{
    chat_store::{ChatStore, ContextUsage, DailyUsage, ModelUsage, StatsRange, ToolUsage},
    error::ErrorService,
    job_store::{JobKind, JobRecord, JobStatus, JobStore},
    llm::{LlamaServerStats, LlmProviders, RunningModel},
    settings_store::SettingsStore,
};

const DEFAULT_DAYS: u32 = 30;
const MAX_DAYS: u32 = 365;
const MAX_MONTHS: u32 = 12;

/// How one kind of background job went over a range.
pub struct JobKindStats {
    pub kind: &'static str,
    pub total: u64,
    /// Ended on its own with exit code 0 (a sub-agent: ended on its own)
    pub succeeded: u64,
    /// Ended on its own with a non-zero exit code
    pub failed: u64,
    pub killed: u64,
    /// The backend restarted while it ran, so how it ended is unknown
    pub lost: u64,
    /// The average run time of the jobs that have finished, in seconds
    pub average_seconds: Option<f64>,
}

pub struct Breakdown {
    pub models: Vec<ModelUsage>,
    pub tools: Vec<ToolUsage>,
}

pub struct ActivitySummary {
    /// The user's own messages per hour of each day of the range, oldest day first
    pub hourly: Vec<(NaiveDate, [u32; 24])>,
    pub chats_per_day: Vec<(NaiveDate, u32)>,
    pub jobs: Vec<JobKindStats>,
}

pub struct ContextSummary {
    /// The context window the agent runs under, in tokens
    pub context_length: u64,
    pub usage: ContextUsage,
}

/// What the model backend reports it is running; `reachable: false` when it didn't answer.
pub struct ServerSnapshot {
    pub reachable: bool,
    /// Why the managed llama.cpp isn't answering when it is the backend asked: `stopped`, `starting`,
    /// `failed` or `not_installed`
    pub state: Option<String>,
    pub detail: Option<String>,
    pub models: Vec<RunningModel>,
    pub llama_server: Option<LlamaServerStats>,
}

/// Everything the stats endpoints work out that isn't shaping an HTTP response: which days a
/// request means (in the caller's own timezone), the numbers drawn from the chat and job stores,
/// and what the model backend says it is doing. The routes only turn the result into the API's
/// types. Errors are plain `ErrorService`, like `PromptFacade`'s — each store's own errors
/// already convert into it.
#[derive(Clone)]
pub struct StatsFacade {
    chat_store: Arc<ChatStore>,
    job_store: Arc<JobStore>,
    settings_store: Arc<SettingsStore>,
    providers: LlmProviders,
    runtime: Arc<LlamaRuntime>,
    context_length: u64,
}

impl StatsFacade {
    pub fn new(
        chat_store: Arc<ChatStore>,
        job_store: Arc<JobStore>,
        settings_store: Arc<SettingsStore>,
        providers: LlmProviders,
        runtime: Arc<LlamaRuntime>,
        context_length: u64,
    ) -> Self {
        Self { chat_store, job_store, settings_store, providers, runtime, context_length }
    }

    fn ended_cleanly(job: &JobRecord) -> bool {
        job.status == JobStatus::Exited && job.exit_code.unwrap_or(0) == 0
    }

    fn summarize_jobs(kind: &'static str, jobs: &[&JobRecord]) -> JobKindStats {
        let durations: Vec<f64> = jobs
            .iter()
            .filter_map(|job| Some((job.finished_at? - job.started_at).num_milliseconds() as f64 / 1000.0))
            .collect();

        JobKindStats {
            kind,
            total: jobs.len() as u64,
            succeeded: jobs.iter().filter(|job| Self::ended_cleanly(job)).count() as u64,
            failed: jobs.iter().filter(|job| job.status == JobStatus::Exited && !Self::ended_cleanly(job)).count() as u64,
            killed: jobs.iter().filter(|job| job.status == JobStatus::Killed).count() as u64,
            lost: jobs.iter().filter(|job| job.status == JobStatus::Lost).count() as u64,
            average_seconds: (!durations.is_empty()).then(|| durations.iter().sum::<f64>() / durations.len() as f64),
        }
    }

    /// The span a request means: the last `months` calendar months if given, else the last `days`
    /// days (30 when neither is). The counts are clamped because they are client input and a huge
    /// one would overflow the date math.
    async fn range(&self, user_id: i64, days: Option<u32>, months: Option<u32>) -> Result<StatsRange, ErrorService> {
        let timezone = self.settings_store.settings(user_id).await?.timezone.unwrap_or(0);
        Ok(match months {
            Some(months) => StatsRange::last_months(months.clamp(1, MAX_MONTHS), timezone),
            None => StatsRange::last_days(days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS), timezone),
        })
    }

    /// Replies and what they cost per day of the range, quiet days included
    pub async fn usage(&self, user_id: i64, days: Option<u32>, months: Option<u32>) -> Result<Vec<DailyUsage>, ErrorService> {
        let range = self.range(user_id, days, months).await?;
        Ok(self.chat_store.usage_daily(user_id, range).await?)
    }

    pub async fn breakdown(&self, user_id: i64, days: Option<u32>, months: Option<u32>) -> Result<Breakdown, ErrorService> {
        let range = self.range(user_id, days, months).await?;
        Ok(Breakdown {
            models: self.chat_store.usage_by_model(user_id, range).await?,
            tools: self.chat_store.usage_by_tool(user_id, range).await?,
        })
    }

    /// When the user is active, the chats they started, and how their background jobs went
    pub async fn activity(&self, user_id: i64, days: Option<u32>, months: Option<u32>) -> Result<ActivitySummary, ErrorService> {
        let range = self.range(user_id, days, months).await?;
        let activity = self.chat_store.activity(user_id, range).await?;

        // A job belongs to a chat, not to a user, so the user's chats scope the query
        let chat_ids = self.chat_store.chat_ids_of(user_id).await?;
        let jobs = self.job_store.started_since(&chat_ids, range.since()).await?;
        let commands: Vec<&JobRecord> = jobs.iter().filter(|job| job.kind == JobKind::Process).collect();
        let sub_agents: Vec<&JobRecord> = jobs.iter().filter(|job| job.kind != JobKind::Process).collect();

        Ok(ActivitySummary {
            hourly: activity.hourly,
            chats_per_day: activity.chats_per_day,
            jobs: vec![Self::summarize_jobs("process", &commands), Self::summarize_jobs("agent", &sub_agents)],
        })
    }

    /// How full the user's biggest chats are against the context window
    pub async fn context(&self, user_id: i64) -> Result<ContextSummary, ErrorService> {
        Ok(ContextSummary { context_length: self.context_length, usage: self.chat_store.context_usage(user_id).await? })
    }

    /// What the model backend has loaded right now. An unreachable backend is an answer, not an
    /// error: the page shows "offline" instead of failing, and the cause is already logged where
    /// the request fails.
    pub async fn server(&self) -> ServerSnapshot {
        // The managed llama.cpp has states of its own: a stopped or missing one is not "offline"
        let status = self.runtime.status();
        if matches!(status.state.as_str(), "stopped" | "starting" | "failed" | "not_installed") {
            return ServerSnapshot { reachable: false, state: Some(status.state), detail: status.detail, models: vec![], llama_server: None };
        }
        match self.providers.default_provider().running_models().await {
            Ok(running) => ServerSnapshot { reachable: true, state: None, detail: None, models: running.models, llama_server: running.llama_server },
            Err(_) => ServerSnapshot { reachable: false, state: None, detail: None, models: vec![], llama_server: None },
        }
    }
}
