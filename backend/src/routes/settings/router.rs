use std::sync::Arc;

use axum::{routing::get, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::get_tools::*;
use super::set_tools::*;
use super::{get::*, set::*, system_prompt_get::*, system_prompt_set::*};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_settings).post(set_settings))
        .route("/system-prompt", get(get_system_prompt).post(set_system_prompt))
        .route("/tools", get(get_user_tools).post(set_user_tools))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_settings, set_settings, get_system_prompt, set_system_prompt, get_user_tools, set_user_tools),
    components(schemas(
        crate::services::settings_store::Settings,
        crate::services::settings_store::SettingsUpdate,
        crate::services::settings_store::SystemPromptOut,
        crate::services::settings_store::SystemPromptUpdate,
        UserToolChoice,
        SetUserToolChoice,
    )),
)]
pub struct ApiDoc;
