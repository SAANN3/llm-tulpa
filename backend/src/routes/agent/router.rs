use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::answer::*;
use super::regenerate::*;
use super::running::*;
use super::start_turn::*;
use super::stop::*;
use super::turn_state::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/turn", post(start_turn).get(turn_state))
        .route("/runs", get(running))
        .route("/regenerate", post(regenerate))
        .route("/answer", post(answer))
        .route("/stop", post(stop))
}

#[derive(OpenApi)]
#[openapi(
    paths(start_turn, turn_state, running, regenerate, answer, stop),
    components(schemas(
        StartTurnRequest,
        StartTurnOut,
        RegenerateRequest,
        AnswerRequest,
        StopRequest,
        RunningChat,
        crate::facade::agent::TurnState,
        crate::facade::agent::TurnStatus,
        crate::facade::agent::RunEnded,
        crate::facade::agent::Decision,
        crate::facade::agent::Allowance,
        crate::facade::agent::AgentToolCall,
        crate::facade::agent::AgentToolPermission,
        crate::facade::agent::AgentScopeGrant,
        crate::services::event_bus::RunEndReason,
    )),
)]
pub struct ApiDoc;
