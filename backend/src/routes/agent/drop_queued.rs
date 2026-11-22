use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DropQueuedQuery {
    chat_id: i64,
    id: i64,
}

/// Drops one prompt waiting behind the chat's current turn. The run in progress is left alone.
#[utoipa::path(
    delete,
    path = "/api/agent/queue",
    tag = "agent",
    params(DropQueuedQuery),
    responses(
        (status = 204, description = "The waiting prompt was dropped"),
        (status = 404, description = "No such chat, or the prompt is no longer waiting", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn drop_queued(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DropQueuedQuery>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, query.chat_id).await?;
    services.agent.drop_queued(query.chat_id, query.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
