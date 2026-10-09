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
pub(crate) struct UsageDayOut {
    /// The day in the caller's timezone, `YYYY-MM-DD`
    day: String,
    /// Assistant replies that day (those that carry token counts)
    replies: u64,
    /// Prompt tokens summed over the replies — the whole context is re-sent with every call,
    /// so this grows with chat length, not with how much new text there was
    prompt_tokens: i64,
    /// Tokens the model generated
    eval_tokens: i64,
    /// Replies that carry the generation timing (recorded since timings were stored), with the
    /// tokens they generated and the milliseconds that took: `timed_eval_tokens / eval_ms` is the
    /// day's generation speed, over exactly the replies it can be measured for
    timed_replies: u64,
    timed_eval_tokens: i64,
    eval_ms: i64,
    /// Calls whose backend reports how many prompt tokens it actually evaluated, with those
    /// tokens and the milliseconds that took: the prompt-processing speed. Calls served from the
    /// server's prompt cache evaluate little and are in here at their true, small size.
    processed_calls: u64,
    processed_tokens: i64,
    processed_ms: i64,
    /// Time spent loading models, in milliseconds
    load_ms: i64,
    /// The middle and 95th-percentile length of a whole call (request to response), in
    /// milliseconds; null on a day without calls
    call_ms_median: Option<i64>,
    call_ms_p95: Option<i64>,
    /// Calls of 30 seconds or more — in practice, a prompt the server had to evaluate again
    slow_calls: u64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct UsageResponse {
    /// Every day of the range, oldest first, quiet days included
    days: Vec<UsageDayOut>,
    /// The average whole call (request to response) over the range, in milliseconds; null without calls
    call_ms_mean: Option<i64>,
    /// Starts of the model server over the range, server-wide (a chat, a one-shot prompt or a load from the
    /// Models page asked for each), and the median time to ready in milliseconds. llama.cpp only: Ollama loads
    /// its models itself.
    loads: u64,
    load_ms_median: Option<i64>,
}

/// The calling user's usage and speed per day over the last `days` days — every chat of theirs,
/// including sub-agent chats and deleted ones.
#[utoipa::path(
    get,
    path = "/api/stats/usage",
    tag = "stats",
    params(StatsQuery),
    responses(
        (status = 200, description = "Usage per day", body = UsageResponse),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn usage(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<StatsQuery>,
) -> Result<Json<UsageResponse>, ErrorService> {
    let services = state.services().await?;
    let overview = services.stats.usage(auth.id, query.days, query.months).await?;
    let summary = overview.summary;
    let days = summary
        .days
        .into_iter()
        .map(|day| UsageDayOut {
            day: day.day.to_string(),
            replies: day.replies,
            prompt_tokens: day.prompt_tokens,
            eval_tokens: day.eval_tokens,
            timed_replies: day.timed_replies,
            timed_eval_tokens: day.timed_eval_tokens,
            eval_ms: day.eval_ms,
            processed_calls: day.processed_calls,
            processed_tokens: day.processed_tokens,
            processed_ms: day.processed_ms,
            load_ms: day.load_ms,
            call_ms_median: day.call_ms_median,
            call_ms_p95: day.call_ms_p95,
            slow_calls: day.slow_calls,
        })
        .collect();

    Ok(Json(UsageResponse { days, call_ms_mean: summary.call_ms_mean, loads: overview.loads.count, load_ms_median: overview.loads.median_ms }))
}
