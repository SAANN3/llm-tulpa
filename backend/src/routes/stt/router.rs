use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::benchmark::*;
use super::install::*;
use super::remove::*;
use super::session_audio::*;
use super::session_cancel::*;
use super::session_finish::*;
use super::session_start::*;
use super::status::*;
use super::transcribe::*;
use super::tuning::*;

/// Five minutes of 16-bit 16 kHz audio is 9.6 MB; the default body limit of 2 MB would cut a whole recording
const RECORDING_LIMIT: usize = 12 * 1024 * 1024;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(status))
        .route("/models", post(install).delete(remove))
        .route("/sessions", post(start_session))
        .route("/sessions/{id}", delete(session_cancel))
        .route("/sessions/{id}/audio", post(session_audio))
        .route("/sessions/{id}/finish", post(session_finish))
        .route("/tuning", post(set_tuning))
        .route("/benchmark", post(benchmark))
        .route("/transcribe", post(transcribe).layer(DefaultBodyLimit::max(RECORDING_LIMIT)))
}

#[derive(OpenApi)]
#[openapi(
    paths(status, install, remove, start_session, session_audio, session_finish, session_cancel, transcribe, set_tuning, benchmark),
    components(schemas(
        SttStatusOut,
        InstallRequest,
        SessionOut,
        TextOut,
        crate::services::stt::SttModel,
        crate::services::stt::SttTask,
        crate::services::stt::SttTuning,
        crate::services::stt::SttBenchmark,
        crate::services::stt::SttBenchModel,
        crate::services::stt::SttBenchRun,
    )),
)]
pub struct ApiDoc;
