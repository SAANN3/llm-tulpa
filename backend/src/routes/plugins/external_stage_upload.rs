use std::sync::Arc;

use axum::{extract::Multipart, extract::State, http::StatusCode, Json};
use utoipa::ToSchema;

use crate::{plugins::external::installer::StagedPlugin, routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Doc-only shape of the multipart body: the handler reads the stream
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct StageUploadRequest {
    /// The `.tulpa` file (a zip with `plugin.toml` at its top)
    #[schema(value_type = String)]
    file: Vec<u8>,
}

/// First step of installing a plugin from a file: the `.tulpa` file is unpacked into a staging area and described (its manifest,
/// its README, its files, what installing it would do), and nothing of it runs. The answer carries the `id` that
/// `POST /api/plugins/external/install` takes once the owner has looked. A staged plugin that is not installed is removed after an hour.
#[utoipa::path(
    post,
    path = "/api/plugins/external/stage/upload",
    tag = "plugins",
    request_body(content = StageUploadRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "The plugin, unpacked and described", body = StagedPlugin),
        (status = 400, description = "Not a plugin file, or one that breaks a rule (a path that leaves its folder, a link, too large)", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn stage_external_upload(State(state): State<Arc<AppState>>, _owner: OwnerUser, mut multipart: Multipart) -> Result<Json<StagedPlugin>, ErrorService> {
    let bad = |what: String| ErrorService::new(StatusCode::BAD_REQUEST, what);
    let mut file = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| bad(format!("invalid multipart body: {e}")))? {
        if field.name() == Some("file") {
            file = Some(field.bytes().await.map_err(|e| bad(format!("could not read the file: {e}")))?.to_vec());
        }
    }
    let bytes = file.ok_or_else(|| bad("no file was sent".to_string()))?;
    Ok(Json(state.services().await?.external_plugins.stage_file(bytes).await?))
}
