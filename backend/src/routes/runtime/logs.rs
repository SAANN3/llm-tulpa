use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::OwnerUser, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct LogsQuery {
    /// Only lines from this index on (the `next` of the previous reply)
    since: Option<usize>,
    /// At most this many of the latest lines (default 300)
    limit: Option<usize>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct LogsOut {
    /// The index of the first line returned
    first: usize,
    /// What to pass as `since` to get only what comes after these lines
    next: usize,
    lines: Vec<String>,
}

/// Owner-only. The latest output of the model server since it was last started: the layers it put on
/// the GPU, the memory it took, and the reason it gave when it failed.
#[utoipa::path(
    get,
    path = "/api/runtime/logs",
    tag = "runtime",
    params(LogsQuery),
    responses(
        (status = 200, description = "Log lines", body = LogsOut),
        (status = 403, description = "Only the owner can read the server log", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn logs(State(state): State<Arc<AppState>>, _owner: OwnerUser, Query(query): Query<LogsQuery>) -> Json<LogsOut> {
    let (first, lines) = state.runtime.logs(query.since, query.limit.unwrap_or(300).min(2000));
    Json(LogsOut { first, next: first + lines.len(), lines })
}
