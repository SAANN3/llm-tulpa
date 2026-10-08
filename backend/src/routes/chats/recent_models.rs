use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct RecentModelsQuery {
    /// How many to return, 1 to 20; 3 when omitted.
    limit: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RecentModelOut {
    pub(crate) provider: String,
    pub(crate) model: String,
    /// The name the owner gave the model, when it has one.
    pub(crate) display_name: Option<String>,
    /// The launch profile the chat runs the model under; `null` for an Ollama model.
    pub(crate) launch_profile_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RecentModelsResponse {
    pub(crate) models: Vec<RecentModelOut>,
}

/// The models (with their launch profiles) the caller's most recently active chats are set to, newest first,
/// each once. Sub-agent and messaging-plugin chats don't count.
#[utoipa::path(
    get,
    path = "/api/chats/recent_models",
    tag = "chats",
    params(RecentModelsQuery),
    responses(
        (status = 200, description = "The recently used models, newest first", body = RecentModelsResponse),
    ),
)]
pub async fn recent_models(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<RecentModelsQuery>,
) -> Result<Json<RecentModelsResponse>, ErrorService> {
    let services = state.services().await?;
    let limit = query.limit.unwrap_or(3).clamp(1, 20);
    let models = services.chat_store.recent_models(auth.id, limit).await?;

    Ok(Json(RecentModelsResponse {
        models: models
            .into_iter()
            .map(|recent| RecentModelOut {
                provider: recent.model.provider,
                model: recent.model.name,
                display_name: recent.model.display_name,
                launch_profile_id: recent.launch_profile_id,
            })
            .collect(),
    }))
}
