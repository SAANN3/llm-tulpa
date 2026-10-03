use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::devices::*;
use super::hardware::*;
use super::install::*;
use super::install_status::*;
use super::load::*;
use super::logs::*;
use super::models::*;
use super::rebind_chats::*;
use super::register::*;
use super::setup_complete::*;
use super::status::*;
use super::stop::*;
use super::test::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(status))
        .route("/load", post(load))
        .route("/stop", post(stop))
        .route("/logs", get(logs))
        .route("/devices", get(devices))
        .route("/hardware", get(hardware))
        .route("/install", get(install_status).post(install))
        .route("/test", post(test))
        .route("/models", get(models).post(register))
        .route("/setup-complete", post(setup_complete))
        .route("/rebind-chats", post(rebind_chats))
}

#[derive(OpenApi)]
#[openapi(
    paths(status, load, stop, logs, devices, hardware, install_status, install, test, models, register, setup_complete, rebind_chats),
    components(schemas(
        LoadRequest,
        LogsOut,
        DevicesOut,
        TestRequest,
        ManagedModelOut,
        ManagedModelsOut,
        RegisterRequest,
        RegisterOut,
        crate::services::llama_runtime::RuntimeStatus,
        crate::services::llama_runtime::LoadFacts,
        crate::services::llama_runtime::MemoryBuffer,
        crate::facade::launch::SpeedTest,
        InstallStatusOut,
        RebindRequest,
        RebindOut,
        crate::services::llama_install::Hardware,
        crate::services::llama_install::Gpu,
        crate::services::llama_install::MissingLib,
        crate::services::llama_install::InstallRequest,
        crate::services::llama_install::Channel,
        crate::services::llama_install::InstallTask,
        crate::services::llama_install::Installed,
    )),
)]
pub struct ApiDoc;
