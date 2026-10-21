use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use super::session_audio::TextOut;
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// Ends a live voice input: the audio not yet settled is read once more and the whole text comes back. The voice input
/// is closed either way.
#[utoipa::path(
    post,
    path = "/api/stt/sessions/{id}/finish",
    tag = "stt",
    params(("id" = String, Path, description = "The voice input's id")),
    responses(
        (status = 200, description = "The whole text", body = TextOut),
        (status = 404, description = "No such voice input", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn session_finish(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>) -> Result<Json<TextOut>, ErrorService> {
    Ok(Json(TextOut { text: state.stt.finish(auth.id, &id).await? }))
}
