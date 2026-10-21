use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, stt::pcm16_to_f32},
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub(crate) struct TextOut {
    pub(super) text: String,
}

/// Adds a chunk of audio to a live voice input and answers with the text so far. The body is raw 16-bit little-endian
/// mono samples at 16 kHz; a chunk of half a second is a good size. The text is as of the last read: reading
/// goes on in the background, and the answer to the next chunk has the newer text.
#[utoipa::path(
    post,
    path = "/api/stt/sessions/{id}/audio",
    tag = "stt",
    params(("id" = String, Path, description = "The voice input's id")),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "The text so far", body = TextOut),
        (status = 404, description = "No such voice input", body = crate::services::error::ErrorBody),
        (status = 413, description = "It has had five minutes of audio", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn session_audio(State(state): State<Arc<AppState>>, auth: AuthUser, Path(id): Path<String>, body: Bytes) -> Result<Json<TextOut>, ErrorService> {
    let text = state.stt.push(auth.id, &id, pcm16_to_f32(&body)).await?;
    Ok(Json(TextOut { text }))
}
