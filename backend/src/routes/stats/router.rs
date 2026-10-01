use std::sync::Arc;

use axum::{routing::get, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::activity::*;
use super::breakdown::*;
use super::context::*;
use super::server::*;
use super::usage::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/usage", get(usage))
        .route("/breakdown", get(breakdown))
        .route("/activity", get(activity))
        .route("/context", get(context))
        .route("/server", get(server))
}

#[derive(OpenApi)]
#[openapi(
    paths(usage, breakdown, activity, context, server),
    components(schemas(
        UsageDayOut,
        UsageResponse,
        ModelUsageOut,
        ToolUsageOut,
        BreakdownResponse,
        ActivityDayOut,
        ChatsDayOut,
        JobKindOut,
        ActivityResponse,
        ContextChatOut,
        ContextResponse,
        RunningModelOut,
        LlamaServerOut,
        ServerResponse,
    )),
)]
pub struct ApiDoc;
