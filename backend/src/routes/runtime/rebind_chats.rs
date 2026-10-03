use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RebindRequest {
    /// The launch profile every chat on another provider's model should move to
    profile_id: i64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RebindOut {
    moved: u64,
}

/// Owner-only. Moves every chat that runs on an Ollama model onto a launch profile of the managed
/// llama.cpp, for an install that is switching over. The chats' messages are untouched; only the
/// model and profile their next turn uses change.
#[utoipa::path(
    post,
    path = "/api/runtime/rebind-chats",
    tag = "runtime",
    request_body = RebindRequest,
    responses(
        (status = 200, description = "How many chats moved", body = RebindOut),
        (status = 404, description = "No such profile", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn rebind_chats(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<RebindRequest>,
) -> Result<Json<RebindOut>, ErrorService> {
    let services = state.services().await?;
    let from: Vec<i64> = services.model_store.list("ollama").await?.into_iter().map(|m| m.id).collect();
    Ok(Json(RebindOut { moved: services.chat_store.rebind_models(&from, body.profile_id).await? }))
}
