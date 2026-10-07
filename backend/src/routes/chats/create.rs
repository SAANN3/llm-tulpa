use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, services::event_bus::ServerEvent, state::AppState};

use super::get::ChatOut;

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateChatRequest {
    name: String,
    /// Start the chat on this launch profile, and so on its model, instead of the default
    #[serde(default)]
    launch_profile_id: Option<i64>,
    /// Start the chat on this model (for one that has no launch profiles, such as an Ollama model)
    /// instead of the default; ignored when `launch_profile_id` is given
    #[serde(default)]
    model: Option<String>,
    /// The provider of `model`; llama.cpp when left out
    #[serde(default)]
    provider: Option<String>,
    /// Whether the model is sent its tools in this chat; the user's default setting when left out
    #[serde(default)]
    tools_enabled: Option<bool>,
}

/// Creates a new chat with the given name and returns its info. The chat is bound to the
/// user's active model (else the first registered one) at creation, or to the launch profile or model
/// the request names (a model for this chat only: the default is untouched).
#[utoipa::path(
    post,
    path = "/api/chats",
    tag = "chats",
    request_body = CreateChatRequest,
    responses(
        (status = 200, description = "Chat created", body = ChatOut),
        (status = 400, description = "A model that isn't installed", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such launch profile", body = crate::services::error::ErrorBody),
        (status = 409, description = "No model has been selected yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_chat(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateChatRequest>,
) -> Result<Json<ChatOut>, ErrorService> {
    let services = state.services().await?;

    // What the chat is to start on instead of the default, checked before the chat exists
    enum Start {
        Default,
        Profile(i64),
        Model(i64),
    }
    let start = match (body.launch_profile_id, &body.model) {
        (Some(profile_id), _) => {
            services.launch_store.get(profile_id).await?;
            Start::Profile(profile_id)
        }
        (None, Some(model)) => {
            let provider = body.provider.as_deref().unwrap_or(crate::facade::launch::MANAGED_PROVIDER);
            state.require_installed_model(&services, provider, model).await?;
            Start::Model(services.model_store.ensure(provider, model).await?.id)
        }
        (None, None) => Start::Default,
    };

    let tools_enabled = match body.tools_enabled {
        Some(enabled) => enabled,
        None => services.settings_store.use_tools(auth.id).await?,
    };
    let mut chat = services.chat_store.create_chat(auth.id, body.name, tools_enabled).await?;
    let applied = match start {
        Start::Default => Ok(()),
        Start::Profile(profile_id) => services.chat_store.set_launch_profile(chat.id, profile_id).await,
        Start::Model(model_id) => services.chat_store.set_model(chat.id, model_id).await,
    };
    match applied {
        Ok(()) => chat = services.chat_store.chat(chat.id).await?,
        Err(e) => {
            // A chat that didn't get the model asked for would silently run on another one
            let _ = services.chat_store.delete_chat(chat.id).await;
            return Err(e.into());
        }
    }
    let contexts = services.launches.contexts(services.context_length).await?;
    state.events.publish(ServerEvent::ChatCreated { chat_id: chat.id });

    Ok(Json(ChatOut {
        id: chat.id,
        name: chat.name,
        model: chat.model,
        provider: chat.provider,
        created_at: chat.created_at,
        updated_at: chat.updated_at,
        last_prompt_tokens: chat.last_prompt_tokens,
        context_length: contexts.for_profile(chat.launch_profile_id),
        folder_id: chat.folder_id,
        parent_chat_id: chat.parent_chat_id,
        launch_profile_id: chat.launch_profile_id,
        tools_enabled: chat.tools_enabled,
        unseen_end: chat.unseen_end,
    }))
}
