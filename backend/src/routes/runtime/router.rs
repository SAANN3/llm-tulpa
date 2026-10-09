use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::devices::*;
use super::folder::*;
use super::hardware::*;
use super::system::*;
use super::install::*;
use super::install_status::*;
use super::load::*;
use super::logs::*;
use super::models::*;
use super::rebind_chats::*;
use super::register::*;
use super::remove::*;
use super::server_settings::*;
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
        .route("/folder", get(get_folder).post(set_folder))
        .route("/folder/browse", get(browse_folders))
        .route("/server", get(get_server_settings).post(set_server_settings))
        .route("/hardware", get(hardware))
        .route("/system", get(system))
        .route("/install", get(install_status).post(install))
        .route("/test", post(test))
        .route("/models", get(models).post(register).delete(remove_model))
        .route("/setup-complete", post(setup_complete))
        .route("/rebind-chats", post(rebind_chats))
}

#[derive(OpenApi)]
#[openapi(
    paths(status, load, stop, logs, devices, get_server_settings, set_server_settings, get_folder, set_folder, browse_folders, hardware, system, install_status, install, test, models, register, remove_model, setup_complete, rebind_chats),
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
        crate::facade::model_removal::RemovedModel,
        crate::services::llama_runtime::LoadFacts,
        crate::services::llama_runtime::MemoryBuffer,
        crate::facade::placement::Placement,
        crate::services::system_load::SystemSnapshot,
        crate::services::system_load::GpuLoad,
        crate::services::gpu_memory::GpuMemory,
        crate::facade::launch::SpeedTest,
        InstallStatusOut,
        FolderOut,
        crate::services::llama_runtime::Tuning,
        SetFolderRequest,
        crate::services::model_folder::Browsed,
        crate::services::model_folder::FolderEntry,
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
