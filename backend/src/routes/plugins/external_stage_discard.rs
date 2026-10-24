use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use super::external_staged::StagedBody;
use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Throws a staged plugin away without installing it
#[utoipa::path(
    post,
    path = "/api/plugins/external/stage/discard",
    tag = "plugins",
    request_body = StagedBody,
    responses(
        (status = 204, description = "Removed"),
        (status = 404, description = "No such staged plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn discard_staged_external(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<StagedBody>) -> Result<StatusCode, ErrorService> {
    state.services().await?.external_plugins.discard(&body.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
