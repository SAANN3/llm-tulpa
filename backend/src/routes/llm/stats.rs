use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{Days, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

const DEFAULT_DAYS: u32 = 30;
const MAX_DAYS: u32 = 365;

#[derive(Deserialize, IntoParams)]
pub(crate) struct StatsQuery {
    /// How many days to count, today included (default 30, at most 365)
    days: Option<u32>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct UsageDayOut {
    /// The UTC calendar day, `YYYY-MM-DD`
    day: String,
    /// Assistant replies that day (those that carry token counts)
    replies: u64,
    /// Prompt tokens summed over those replies — the whole context is re-sent with every
    /// call, so this grows with chat length, not with how much new text there was
    prompt_tokens: i64,
    /// Tokens the model generated
    eval_tokens: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct StatsResponse {
    /// Only the days that had any activity, oldest first
    days: Vec<UsageDayOut>,
}

/// The calling user's token usage per UTC day over the last `days` days — every chat of
/// theirs, including sub-agent chats and deleted ones.
#[utoipa::path(
    get,
    path = "/api/llm/stats",
    tag = "llm",
    params(StatsQuery),
    responses(
        (status = 200, description = "Usage per day", body = StatsResponse),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn stats(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, ErrorService> {
    // Clamped because the value is client input and a huge one would overflow the date math
    let days = query.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);
    let first_day = Utc::now().date_naive() - Days::new(u64::from(days - 1));
    let since = first_day.and_time(NaiveTime::MIN).and_utc();

    let services = state.services().await?;
    let usage = services.chat_store.usage_by_day(auth.id, since).await?;

    let days = usage
        .into_iter()
        .map(|day| UsageDayOut {
            day: day.day.to_string(),
            replies: day.replies,
            prompt_tokens: day.prompt_tokens,
            eval_tokens: day.eval_tokens,
        })
        .collect();

    Ok(Json(StatsResponse { days }))
}
