use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{facade::agent::NoticeOut, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct JobNoticesRequest {
    chat_id: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct JobNoticesOut {
    /// The notices just persisted, oldest first. Empty if no job has finished since the model was
    /// last told about one.
    notices: Vec<NoticeOut>,
}

/// Persists a notice for each background job or sub-agent of `chat_id` that has finished since the
/// model was last told about one, and returns them — without calling the model. What a client calls
/// after `GET /api/events` says `job_finished` while the chat has no turn running: it shows the
/// notices right away and then calls `POST /api/agent/continue` so the model responds to them, its
/// tool calls driven like any other turn. Safe to call on a stale or duplicate hint, since whether
/// there's anything to report is decided here.
#[utoipa::path(
    post,
    path = "/api/agent/job_notices",
    tag = "agent",
    request_body = JobNoticesRequest,
    responses(
        (status = 200, description = "The notices that were persisted; empty if there was nothing to report", body = JobNoticesOut),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn job_notices(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<JobNoticesRequest>,
) -> Result<Json<JobNoticesOut>, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    let notices = services.agent.flush_notices(body.chat_id).await?;

    Ok(Json(JobNoticesOut { notices }))
}
