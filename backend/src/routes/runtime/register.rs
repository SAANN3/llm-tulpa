use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::models::ManagedModelOut;
use crate::{
    facade::launch::MANAGED_PROVIDER,
    routes::auth::OwnerUser,
    services::{
        error::ErrorService,
        gguf::GgufKind,
        launch_store::LaunchInput,
    },
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RegisterRequest {
    /// The model file, relative to the model directory
    file: String,
    /// What to call it in the UI; the file name when left out
    display_name: Option<String>,
    /// A vision projector in the model directory, to give the model's first profile vision
    projector: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RegisterOut {
    model: ManagedModelOut,
}

/// Owner-only. Makes a `.gguf` file in the model directory a model the backend runs itself. A model
/// registered for the first time gets a first launch profile, with the context sized to free memory
/// and MTP drafting switched on when the file carries the draft head. Registering a file again only
/// updates its display name.
#[utoipa::path(
    post,
    path = "/api/runtime/models",
    tag = "runtime",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registered", body = RegisterOut),
        (status = 400, description = "Not a readable language model file in the model directory", body = crate::services::error::ErrorBody),
        (status = 403, description = "Only the owner can register models", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn register(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterOut>), ErrorService> {
    let services = state.services().await?;
    let info = state.library.gguf_info(&body.file).await?;
    if info.kind != GgufKind::Model {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, format!("'{}' is not a language model (it is a vision projector)", body.file)));
    }
    if let Some(projector) = &body.projector {
        state.library.check_pairing(&body.file, &[state.library.resolve_local(&body.file)?, state.library.resolve_local(projector)?]).await?;
    }

    let model = services.model_store.ensure(MANAGED_PROVIDER, &body.file).await?;
    let display_name = body.display_name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    services.model_store.set_display_name(model.id, display_name.clone()).await?;

    let mut profile_ids: Vec<i64> = services.launch_store.list(Some(model.id)).await?.into_iter().map(|p| p.id).collect();
    if profile_ids.is_empty() {
        let first = LaunchInput { mmproj_file: body.projector, mtp: info.has_mtp(), ..LaunchInput::default() };
        profile_ids.push(services.launch_store.create(model.id, first).await?.id);
    }
    Ok((
        StatusCode::CREATED,
        Json(RegisterOut { model: ManagedModelOut { id: model.id, file_missing: false, file: model.name, display_name, profile_ids } }),
    ))
}
