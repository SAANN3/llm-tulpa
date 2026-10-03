use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetProfileRequest {
    chat_id: i64,
    /// The launch profile this chat should run on from now on; it brings its model along.
    profile_id: i64,
}

/// Rebinds a chat to a launch profile, and so to the profile's model. Takes effect on the chat's next
/// turn: the profile is read from the chat on every call.
#[utoipa::path(
    post,
    path = "/api/chats/profile",
    tag = "chats",
    request_body = SetProfileRequest,
    responses(
        (status = 204, description = "Profile set"),
        (status = 404, description = "No such chat or profile", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetProfileRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.chat_store.set_launch_profile(body.chat_id, body.profile_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
