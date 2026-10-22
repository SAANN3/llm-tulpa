use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{enable::*, get_help::*, list::*, proxy::plugin_proxy, reset_chat::*, set_settings::*, settings_schema::*, tab::*, tab_action::*, tab_upload::*, tabs::*};

/// The most a tab's upload may carry: a zip or a folder of notes and scripts
const MAX_TAB_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

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
    paths(list_plugins, set_plugin_settings, set_plugin_enabled, plugin_settings_schema, plugin_help, reset_plugin_chat, plugin_tabs, plugin_tab, plugin_tab_action, plugin_tab_upload),
    components(schemas(
        crate::plugins::registry::PluginInfo,
        SetPluginSettingsBody,
        SetPluginEnabledBody,
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
