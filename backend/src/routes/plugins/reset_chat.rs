use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub struct ResetPluginChatBody {
    pub plugin_name: String,
    pub plugin_subname: String,
    /// The provider's own identifier for the conversation — a Telegram chat id, etc.
    pub plugin_chat_id: String,
}

/// Wipes a plugin-linked chat's message history (and compaction summary) while keeping
/// the chat itself and its external mapping intact, so the same conversation keeps
/// working with a clean slate — useful for retesting behavior (a system prompt change,
/// say) without a stale refusal or off-topic reply from earlier still sitting in the
/// history the model gets replayed on every turn. Mirrors `ChatStore::clear_messages`
/// — see there for why this isn't just `DELETE /api/chats` (a plugin chat's external
/// mapping can't be safely re-created after that route's soft delete).
#[utoipa::path(
    post,
    path = "/api/plugins/reset_chat",
    tag = "plugins",
    request_body = ResetPluginChatBody,
    responses(
        (status = 204, description = "Chat history cleared"),
        (status = 404, description = "No chat mapped to this plugin_name/plugin_subname/plugin_chat_id", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn reset_plugin_chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ResetPluginChatBody>,
) -> Result<StatusCode, ErrorService> {
    let chat = state
        .chat_store
        .find_by_plugin_mapped_id(&body.plugin_name, &body.plugin_subname, &body.plugin_chat_id)
        .await?
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no chat mapped to this plugin chat id"))?;

    state.chat_store.clear_messages(chat.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
