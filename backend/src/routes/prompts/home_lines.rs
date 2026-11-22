use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{facade::prompt::PromptFacade, routes::auth::AuthUser};

/// The built-in greeting and placeholder lines. The home page picks from these when the user has not written their own.
#[derive(Serialize, ToSchema)]
pub(crate) struct HomeLinesOut {
    pub greetings: Vec<String>,
    pub placeholders: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/prompts/home_lines",
    tag = "prompts",
    responses((status = 200, description = "The built-in greeting and placeholder lines", body = HomeLinesOut)),
)]
pub async fn home_lines(_auth: AuthUser) -> Json<HomeLinesOut> {
    Json(HomeLinesOut {
        greetings: PromptFacade::default_greetings().iter().map(|line| (*line).to_string()).collect(),
        placeholders: PromptFacade::default_placeholders().iter().map(|line| (*line).to_string()).collect(),
    })
}
