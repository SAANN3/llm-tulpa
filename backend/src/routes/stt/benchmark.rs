use std::sync::Arc;

use axum::{body::Bytes, extract::State, http::StatusCode, Json};

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, stt::{pcm16_to_f32, SttBenchmark}},
    state::AppState,
};

/// Owner-only. Measures how fast every installed model reads on this machine at several thread counts, on a recording
/// the owner has just made: the body is raw 16-bit little-endian mono samples at 16 kHz, 3 to 15 seconds. The recording
/// is kept in memory for the measurement only. Runs in the background and takes all the cores; watch it with
/// `GET /api/stt`.
#[utoipa::path(
    post,
    path = "/api/stt/benchmark",
    tag = "stt",
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 202, description = "Started", body = SttBenchmark),
        (status = 400, description = "The recording is too short or too long", body = crate::services::error::ErrorBody),
        (status = 409, description = "No model installed, a voice input is open, or a measurement is running", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn benchmark(State(state): State<Arc<AppState>>, auth: OwnerUser, body: Bytes) -> Result<(StatusCode, Json<SttBenchmark>), ErrorService> {
    let (_, language) = state.services().await?.settings_store.stt_choices(auth.0.id).await?;
    let started = state.stt.start_benchmark(pcm16_to_f32(&body), language.unwrap_or_default())?;
    Ok((StatusCode::ACCEPTED, Json(started)))
}
