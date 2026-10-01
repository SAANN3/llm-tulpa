use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use super::query::StatsQuery;
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct ActivityDayOut {
    /// The day in the caller's timezone, `YYYY-MM-DD`
    day: String,
    /// The caller's own messages in each hour of the day (24 entries, midnight first)
    hours: Vec<u32>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatsDayOut {
    day: String,
    chats: u32,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct JobKindOut {
    /// `process` (a shell command) or `agent` (a sub-agent)
    kind: String,
    total: u64,
    /// Ended on its own with exit code 0 (a sub-agent: ended on its own)
    succeeded: u64,
    /// Ended on its own with a non-zero exit code
    failed: u64,
    killed: u64,
    /// The backend restarted while it ran, so how it ended is unknown
    lost: u64,
    /// The average run time of the jobs that have finished, in seconds
    average_seconds: Option<f64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ActivityResponse {
    /// Every day of the range, oldest first, with the user's own messages per hour — what a view
    /// sums into hours, blocks of hours, or whole days
    days: Vec<ActivityDayOut>,
    /// Chats started per day, every day of the range
    chats_per_day: Vec<ChatsDayOut>,
    /// Background jobs started in the range, per kind
    jobs: Vec<JobKindOut>,
}

/// When the calling user is active, how many chats they started, and how their background jobs
/// went, over the last `days` days.
#[utoipa::path(
    get,
    path = "/api/stats/activity",
    tag = "stats",
    params(StatsQuery),
    responses(
        (status = 200, description = "Activity", body = ActivityResponse),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn activity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<StatsQuery>,
) -> Result<Json<ActivityResponse>, ErrorService> {
    let services = state.services().await?;
    let activity = services.stats.activity(auth.id, query.days, query.months).await?;

    Ok(Json(ActivityResponse {
        days: activity
            .hourly
            .into_iter()
            .map(|(day, hours)| ActivityDayOut { day: day.to_string(), hours: hours.to_vec() })
            .collect(),
        chats_per_day: activity
            .chats_per_day
            .into_iter()
            .map(|(day, chats)| ChatsDayOut { day: day.to_string(), chats })
            .collect(),
        jobs: activity
            .jobs
            .into_iter()
            .map(|job| JobKindOut {
                kind: job.kind.to_string(),
                total: job.total,
                succeeded: job.succeeded,
                failed: job.failed,
                killed: job.killed,
                lost: job.lost,
                average_seconds: job.average_seconds,
            })
            .collect(),
    }))
}
