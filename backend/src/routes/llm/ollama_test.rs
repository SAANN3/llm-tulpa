use std::sync::Arc;

use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, llm::OllamaService},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct TestOllamaRequest {
    url: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct TestOllamaOut {
    reachable: bool,
    /// How many models are installed, when it answered
    models: Option<usize>,
    /// Why it didn't answer
    reason: Option<String>,
}

/// Owner-only. Asks the Ollama at an address for its models, without changing any setting, so the
/// address can be checked before it is saved.
#[utoipa::path(
    post,
    path = "/api/llm/ollama/test",
    tag = "llm",
    request_body = TestOllamaRequest,
    responses(
        (status = 200, description = "Whether it answered", body = TestOllamaOut),
        (status = 400, description = "Not a usable address", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn test_ollama(
    axum::extract::State(_state): axum::extract::State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<TestOllamaRequest>,
) -> Result<Json<TestOllamaOut>, ErrorService> {
    let url = OllamaService::normalize_url(&body.url)?;
    Ok(Json(match OllamaService::probe(&url).await {
        Ok(models) => TestOllamaOut { reachable: true, models: Some(models), reason: None },
        Err(reason) => TestOllamaOut { reachable: false, models: None, reason: Some(reason) },
    }))
}
