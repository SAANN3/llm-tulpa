use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::launch::SpeedTest, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

const DEFAULT_PROMPT: &str = "Write a short paragraph about why the sky is blue.";

#[derive(Deserialize, ToSchema)]
pub(crate) struct TestRequest {
    profile_id: i64,
    /// What to ask; a short default when left out
    prompt: Option<String>,
}

/// Loads the profile if it isn't (the same rules as `/api/runtime/load`), sends one prompt through it
/// and reports how fast it read the prompt and wrote the reply, and how often MTP drafting guessed
/// right. Measures the real thing, so a profile can be compared to another before a chat uses it.
#[utoipa::path(
    post,
    path = "/api/runtime/test",
    tag = "runtime",
    request_body = TestRequest,
    responses(
        (status = 200, description = "What the test measured", body = SpeedTest),
        (status = 423, description = "Another user has a turn running on the loaded model", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn test(State(state): State<Arc<AppState>>, auth: AuthUser, Json(body): Json<TestRequest>) -> Result<Json<SpeedTest>, ErrorService> {
    let prompt = body.prompt.filter(|p| !p.trim().is_empty()).unwrap_or_else(|| DEFAULT_PROMPT.to_string());
    Ok(Json(state.services().await?.launches.speed_test(auth.id, body.profile_id, prompt).await?))
}
