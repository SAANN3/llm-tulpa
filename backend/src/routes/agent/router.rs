use std::sync::Arc;

use axum::{routing::post, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::allow_scope::*;
use super::can_use_tool::*;
use super::chat::*;
use super::continue_chat::*;
use super::use_tool::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/chat", post(chat))
        .route("/continue", post(continue_chat))
        .route("/can_use_tool", post(can_use_tool))
        .route("/use_tool", post(use_tool))
        .route("/allow_scope", post(allow_scope))
}

#[derive(OpenApi)]
#[openapi(
    paths(chat, continue_chat, can_use_tool, use_tool, allow_scope),
    components(schemas(
        ChatRequest,
        ContinueChatRequest,
        CanUseToolRequest,
        UseToolRequest,
        AllowScopeRequest,
        AllowScopeOut,
        crate::facade::agent::ChatOut,
        crate::facade::agent::CanUseTool,
        crate::facade::agent::UseToolOut,
        crate::facade::agent::AgentToolCall,
        crate::facade::agent::AgentToolPermission,
        crate::facade::agent::AgentScopeGrant,
    )),
)]
pub struct ApiDoc;
