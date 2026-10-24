use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{enable::*, external_export::*, external_install::*, external_list::*, external_load::*, external_stage_discard::*, external_stage_git::*, external_stage_upload::*, external_staged::*, external_uninstall::*, external_unload::*, get_help::*, list::*, proxy::plugin_proxy, reset_chat::*, set_settings::*, settings_schema::*, tab::*, tab_action::*, tab_upload::*, tabs::*};

/// The most a tab's upload may carry: a zip or a folder of notes and scripts
const MAX_TAB_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
/// The most a `.tulpa` file may be: a plugin with its sources, or a prebuilt binary or two
const MAX_PLUGIN_UPLOAD_BYTES: usize = 256 * 1024 * 1024;

/// The plugin registry's own management routes (list/settings/enable/settings_schema/help/
/// reset_chat), all owner-only, plus a fallback that proxies every other path into the
/// matching plugin's own router (see `plugin_proxy`). The registry lives inside the swappable
/// `AppServices`, so nothing here is built from it: the proxy resolves the plugin per request,
/// which is what lets this router be an ordinary synchronous one like every other domain's —
/// and mounted (and authenticated) the same way — even when the backend started before a
/// database was configured.
///
/// The registry management routes live here rather than on any individual `Plugin`'s own
/// `api_router()` — they're not about one plugin instance, they're about the registry itself
/// (which plugins exist, their settings, their enabled state).
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_plugins))
        .route("/settings", post(set_plugin_settings))
        .route("/enable", post(set_plugin_enabled))
        .route("/settings_schema", get(plugin_settings_schema))
        .route("/help", get(plugin_help))
        .route("/reset_chat", post(reset_plugin_chat))
        .route("/external", get(list_external_plugins))
        .route("/external/load", post(load_external_plugin))
        .route("/external/unload", post(unload_external_plugin))
        .route("/external/stage", get(get_staged_external))
        .route("/external/stage/file", get(get_staged_external_file))
        .route("/external/stage/upload", post(stage_external_upload).layer(DefaultBodyLimit::max(MAX_PLUGIN_UPLOAD_BYTES)))
        .route("/external/stage/git", post(stage_external_git))
        .route("/external/stage/discard", post(discard_staged_external))
        .route("/external/install", post(install_external_plugin))
        .route("/external/tasks", get(get_external_install))
        .route("/external/uninstall", post(uninstall_external_plugin))
        .route("/external/export", get(export_external_plugin))
        // The tabs are for every user, unlike the routes above: they are the plugins' own screens in Settings
        .route("/tabs", get(plugin_tabs))
        .route("/tab", get(plugin_tab))
        .route("/tab/action", post(plugin_tab_action))
        .route("/tab/upload", post(plugin_tab_upload).layer(DefaultBodyLimit::max(MAX_TAB_UPLOAD_BYTES)))
        .fallback(plugin_proxy)
}

// The plugins' own routes aren't in this document — a plugin's routes aren't known until it's
// registered at runtime, so there's nothing to describe statically.
#[derive(OpenApi)]
#[openapi(
    paths(list_plugins, list_external_plugins, stage_external_upload, stage_external_git, get_staged_external, get_staged_external_file, discard_staged_external, install_external_plugin, get_external_install, uninstall_external_plugin, export_external_plugin, load_external_plugin, unload_external_plugin, set_plugin_settings, set_plugin_enabled, plugin_settings_schema, plugin_help, reset_plugin_chat, plugin_tabs, plugin_tab, plugin_tab_action, plugin_tab_upload),
    components(schemas(
        crate::plugins::registry::PluginInfo,
        SetPluginSettingsBody,
        SetPluginEnabledBody,
        ExternalPluginBody,
        StageUploadRequest,
        StageGitBody,
        StagedBody,
        UninstallBody,
        crate::plugins::external::installer::StagedPlugin,
        crate::plugins::external::installer::StageAction,
        crate::plugins::external::installer::InstallTask,
        crate::plugins::external::installer::TaskState,
        crate::plugins::external::stage::StagedSource,
        crate::plugins::external::stage::StagedFile,
        crate::plugins::external::manager::ExternalPluginView,
        crate::plugins::external::supervisor::Status,
        crate::plugins::external::supervisor::State,
        crate::services::external_plugin_store::PluginState,
        ResetPluginChatBody,
        crate::tools::base::PropertyInfo,
        crate::tools::base::PropertyType,
        crate::plugins::tab::PluginTab,
        crate::plugins::tab::TabLayout,
        crate::plugins::tab::Section,
        crate::plugins::tab::Block,
        crate::plugins::tab::Row,
        crate::plugins::tab::RowSwitch,
        crate::plugins::tab::RowButton,
        crate::plugins::tab::TabPopup,
        crate::plugins::tab::TabActionBody,
        TabUploadRequest,
    )),
)]
pub struct ApiDoc;
