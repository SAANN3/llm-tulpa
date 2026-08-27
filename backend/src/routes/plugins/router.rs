use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{enable::*, get_help::*, list::*, reset_chat::*, set_settings::*, settings_schema::*};

/// Mounts the registry's own management routes (list/settings/enable/settings_schema
/// below) plus every registered plugin's stable proxy (see `PluginRegistry::router`)
/// under `/plugins`. Deliberately unlike every other domain's `router()`: this one is
/// `async` and takes `state` directly, because building the plugin proxy route tree
/// means reading the registry's current key set — the other domains' routes are fixed
/// at compile time, so their `router()` needs neither. For that reason it's *not*
/// nested inside `routes::router::router()` alongside them — `main.rs` calls this
/// separately and merges the result in, keeping that composition function synchronous
/// and uniform for every domain that doesn't need this exception. Called once, before
/// `AppState` is handed to `.with_state()`.
///
/// The registry management routes live here rather than on any individual `Plugin`'s
/// own `api_router()` — they're not about one plugin instance, they're about the
/// registry itself (which plugins exist, their settings, their enabled state), so they
/// belong at this level regardless of which (if any) plugins are actually registered.
///
/// The per-plugin proxy sub-tree never needs rebuilding after this: each plugin's own
/// path is a stable proxy that looks up its live instance fresh per request (see
/// `PluginRegistry::proxy_for`), so a settings change or enable/disable later doesn't
/// require touching this router again.
pub async fn router(state: &Arc<AppState>) -> Router<Arc<AppState>> {
    let plugin_routes: Router = state.plugin_registry.router().await;

    // `.fallback_service`, not `.nest`/`.nest_service` at `"/"` — axum 0.8 rejects
    // nesting a router at the literal root ("Nesting at the root is no longer
    // supported"). `fallback_service` is exactly what's needed anyway: try the static
    // registry routes below first, and only fall through to a per-plugin proxy (e.g.
    // `/messaging/telegram/...`) when nothing above matched.
    Router::new()
        .route("/", get(list_plugins))
        .route("/settings", post(set_plugin_settings))
        .route("/enable", post(set_plugin_enabled))
        .route("/settings_schema", get(plugin_settings_schema))
        .route("/help", get(plugin_help))
        .route("/reset_chat", post(reset_plugin_chat))
        .fallback_service(plugin_routes)
}

#[derive(OpenApi)]
#[openapi(
    paths(list_plugins, set_plugin_settings, set_plugin_enabled, plugin_settings_schema, plugin_help, reset_plugin_chat),
    components(schemas(
        crate::plugins::registry::PluginInfo,
        SetPluginSettingsBody,
        SetPluginEnabledBody,
        ResetPluginChatBody,
        crate::tools::base::PropertyInfo,
        crate::tools::base::PropertyType,
    )),
)]
pub struct ApiDoc;

// No `ApiDoc`/utoipa merge here, unlike the other domains — a plugin's routes aren't
// known until it's registered at runtime, so there's nothing to document statically.
