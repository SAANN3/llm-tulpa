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
pub(crate) struct ModelUsageOut {
    provider: String,
    model: String,
    /// Chats of the user's that used this model in the range
    chats: u64,
    replies: u64,
    eval_tokens: i64,
    /// Generation speed inputs, over the replies that carry a timing (see `/api/stats/usage`)
    timed_eval_tokens: i64,
    eval_ms: i64,
    /// The middle length of a whole call, in milliseconds
    call_ms_median: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ToolUsageOut {
    tool: String,
    calls: u64,
    /// Calls that ran and failed
    failed: u64,
    /// Calls that never ran because the permission system refused them
    denied: u64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct BreakdownResponse {
    /// Busiest first
    models: Vec<ModelUsageOut>,
    /// Most used first
    tools: Vec<ToolUsageOut>,
}

/// How the calling user's usage splits across models and across tools over the last `days` days.
#[utoipa::path(
    get,
    path = "/api/stats/breakdown",
    tag = "stats",
    params(StatsQuery),
    responses(
        (status = 200, description = "Usage per model and per tool", body = BreakdownResponse),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn breakdown(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<StatsQuery>,
) -> Result<Json<BreakdownResponse>, ErrorService> {
    let services = state.services().await?;
    let breakdown = services.stats.breakdown(auth.id, query.days, query.months).await?;

    let models = breakdown
        .models
        .into_iter()
        .map(|model| ModelUsageOut {
            provider: model.provider,
            model: model.model,
            chats: model.chats,
            replies: model.replies,
            eval_tokens: model.eval_tokens,
            timed_eval_tokens: model.timed_eval_tokens,
            eval_ms: model.eval_ms,
            call_ms_median: model.call_ms_median,
        })
        .collect();
    let tools = breakdown
        .tools
        .into_iter()
        .map(|tool| ToolUsageOut { tool: tool.tool, calls: tool.calls, failed: tool.failed, denied: tool.denied })
        .collect();

    Ok(Json(BreakdownResponse { models, tools }))
}
