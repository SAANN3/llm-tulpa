use std::sync::Arc;

use axum::{body::Bytes, extract::State, Json};

use super::session_audio::TextOut;
use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, stt::pcm16_to_f32},
    state::AppState,
};

/// Reads a whole recording on the caller's model and language: the body is raw 16-bit little-endian mono samples at
/// 16 kHz, five minutes at most. For a recording that is already complete; a live input uses the session calls.
#[utoipa::path(
    post,
    path = "/api/stt/transcribe",
    tag = "stt",
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "The text", body = TextOut),
        (status = 409, description = "No model is installed", body = crate::services::error::ErrorBody),
        (status = 413, description = "Longer than five minutes", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn transcribe(State(state): State<Arc<AppState>>, auth: AuthUser, body: Bytes) -> Result<Json<TextOut>, ErrorService> {
    let (chosen, language) = state.services().await?.settings_store.stt_choices(auth.id).await?;
    let model = state.stt.resolve_model(chosen.as_deref())?;
    let text = state.stt.transcribe(model, language.as_deref().unwrap_or(""), pcm16_to_f32(&body)).await?;
    Ok(Json(TextOut { text }))
}
