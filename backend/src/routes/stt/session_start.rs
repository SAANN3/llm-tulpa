use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct SessionOut {
    /// Names the voice input in the calls that follow
    id: String,
    /// The model it runs on
    model: String,
}

/// Opens a live voice input on the caller's model and language (Settings, Voice). Audio goes in with
/// `POST /api/stt/sessions/{id}/audio` and the answer to each chunk is the text so far; `.../finish` ends it with the
/// whole text. One nobody sends to for two minutes is dropped.
#[utoipa::path(
    post,
    path = "/api/stt/sessions",
    tag = "stt",
    responses(
        (status = 201, description = "Opened", body = SessionOut),
        (status = 409, description = "No model is installed", body = crate::services::error::ErrorBody),
        (status = 429, description = "Too many voice inputs are open", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn start_session(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<(StatusCode, Json<SessionOut>), ErrorService> {
    let (chosen, language) = state.services().await?.settings_store.stt_choices(auth.id).await?;
    let model = state.stt.resolve_model(chosen.as_deref())?;
    let id = state.stt.start_session(auth.id, model, language.as_deref().unwrap_or(""))?;
    Ok((StatusCode::CREATED, Json(SessionOut { id, model: model.id().to_string() })))
}
