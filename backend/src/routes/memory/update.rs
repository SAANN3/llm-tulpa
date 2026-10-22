use std::sync::Arc;

use axum::{extract::State, Json};

use super::create::parse_category;
use super::dto::{MemoryEntryOut, UpdateMemoryRequest};
use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// Changes an entry's category and text. The entry is the caller's own from then on: the model never changes or drops it.
#[utoipa::path(
    post,
    path = "/api/memory/update",
    tag = "memory",
    request_body = UpdateMemoryRequest,
    responses(
        (status = 200, description = "Entry changed", body = MemoryEntryOut),
        (status = 400, description = "The text is empty or too long, or the category is not one of the three", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such entry", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn update_memory(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<UpdateMemoryRequest>,
) -> Result<Json<MemoryEntryOut>, ErrorService> {
    let category = parse_category(&body.category)?;
    let entry = state.services().await?.memory_store.update(auth.id, body.id, category, &body.text).await?;
    Ok(Json(entry.into()))
}
