use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use super::dto::{MemoryEntryIn, MemoryEntryOut};
use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, memory_store::Category},
    state::AppState,
};

/// Adds an entry the caller writes themselves. The model never changes or drops it.
#[utoipa::path(
    post,
    path = "/api/memory",
    tag = "memory",
    request_body = MemoryEntryIn,
    responses(
        (status = 201, description = "Entry added", body = MemoryEntryOut),
        (status = 400, description = "The text is empty or too long, or the category is not one of the three", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_memory(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<MemoryEntryIn>,
) -> Result<(StatusCode, Json<MemoryEntryOut>), ErrorService> {
    let category = parse_category(&body.category)?;
    let entry = state.services().await?.memory_store.add(auth.id, category, &body.text).await?;
    Ok((StatusCode::CREATED, Json(entry.into())))
}

pub(super) fn parse_category(value: &str) -> Result<Category, ErrorService> {
    Category::parse(value)
        .ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "the category is one of about_you, preferences and misc"))
}
