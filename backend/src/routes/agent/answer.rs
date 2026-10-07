use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    facade::agent::Decision, routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkChoice, state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct AnswerRequest {
    chat_id: i64,
    /// One answer per pending tool call the user decides on, by its position in `pending` of `GET
    /// /api/agent/turn`. A call with no answer is refused: the model is told it was denied.
    decisions: Vec<Decision>,
    /// Same as on `POST /api/agent/turn`.
    think: Option<ThinkChoice>,
}

/// Answers the permission prompt `chat_id` is waiting at and continues its turn: `permanent` grants what the
/// call asked for now and for later calls in the chat, `only_now` lets that one call through, `deny` refuses
/// it. The pending calls then run in order and the model is asked again, in the background.
#[utoipa::path(
    post,
    path = "/api/agent/answer",
    tag = "agent",
    request_body = AnswerRequest,
    responses(
        (status = 202, description = "The answers are applied and the run continues"),
        (status = 400, description = "An answer points at no pending call, or grants what the call doesn't ask for", body = crate::services::error::ErrorBody),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has a run going on, nothing waiting for an answer, or belongs to a messaging plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn answer(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<AnswerRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.agent.answer(body.chat_id, body.decisions, body.think).await?;
    Ok(StatusCode::ACCEPTED)
}
