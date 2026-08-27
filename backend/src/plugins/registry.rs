use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;
use tower::ServiceExt;
use utoipa::ToSchema;

use super::base::{Plugin, PluginBuilder, PluginError};
use crate::services::plugin_settings_store::PluginSettingsStore;

/// Identifies one plugin instance: (plugin_name, plugin_subname) — e.g.
/// ("messaging", "vk"). `plugin_name` groups instances that share an API shape and
/// settings contract; `plugin_subname` picks the concrete implementation.
pub type PluginKey = (String, String);

/// One entry in `PluginRegistry::list`'s output — everything the frontend's plugin
/// list needs to render a row (name/subname, enabled state, current settings) without
/// exposing the registry's own internal `PluginEntry`.
#[derive(Serialize, ToSchema)]
pub struct PluginInfo {
    pub plugin_name: String,
    pub plugin_subname: String,
    pub enabled: bool,
    #[schema(value_type = Object)]
    pub settings: Option<Value>,
}

struct PluginEntry {
    /// Kept alongside the live instance so a settings change can rebuild it later
    /// (see `PluginBuilder::build`) without the registry needing to know anything
    /// plugin-type-specific.
    builder: Arc<dyn PluginBuilder>,
    /// `None` until settings exist for this plugin — e.g. right after first startup,
    /// before the user has ever filled in VK's token. A plugin is known to the
    /// registry (listed, has a builder/schema to render a settings form from) well
    /// before it necessarily has an instance to run.
    plugin: Option<Arc<dyn Plugin>>,
    /// `plugin.api_router()`, built once at the same time as `plugin` and kept until
    /// the next rebuild — not rebuilt per request. `Router` is cheap to `Clone`
    /// (internally `Arc`-based), so handing one out to a proxy on every request is just
    /// a pointer bump, but building one from scratch involves constructing the whole
    /// route-matching tree, which there's no reason to redo when nothing changed since
    /// the last request. Always `Some` exactly when `plugin` is.
    router: Option<Router>,
    /// Invariant enforced by `register`/`set_enabled`: this can only be `true` when
    /// `plugin` is `Some` — nothing without settings is ever "enabled".
    enabled: bool,
}

/// Holds every registered plugin instance, keyed by `PluginKey`. Route mounting and
/// persisting settings to the database are done (see `store` below and
/// `PluginSettingsStore`); `register`/`update_settings`/`set_enabled` below all call
/// `Plugin::on_enabled`/`on_disabled` themselves wherever a plugin's running state
/// actually changes, so nothing outside this module ever needs to call them directly.
pub struct PluginRegistry {
    entries: RwLock<HashMap<PluginKey, PluginEntry>>,
    /// The only thing outside this module that ever talks to `PluginSettingsStore` —
    /// every read/write goes through `register`/`update_settings`/`set_enabled` below,
    /// so nothing else in the app needs to know the store exists.
    store: Arc<PluginSettingsStore>,
}

impl PluginRegistry {
    pub fn new(store: Arc<PluginSettingsStore>) -> Self {
        Self { entries: RwLock::new(HashMap::new()), store }
    }

    /// Registers a plugin under the builder's own `(plugin_name, plugin_subname)`.
    /// `initial_settings`/`enabled` are only what to fall back to when nothing's been
    /// persisted for this key before — whatever `PluginSettingsStore` actually has
    /// takes precedence, so a restart restores what was really configured last instead
    /// of silently resetting to the caller's own default every time. `initial_settings`
    /// ending up `None` (no persisted settings, and the caller passed none either) is
    /// normal for a plugin that's known to exist but hasn't been configured yet — it's
    /// stored with no live instance and `enabled` forced to `false`, since there's
    /// nothing to run. Passing `enabled: true` with no settings anywhere is a caller
    /// bug, not a state that gets silently corrected — it's rejected instead. If this
    /// restores a plugin that was persisted as enabled (e.g. a Telegram bot that was
    /// running before the last restart), `on_enabled` is called on the freshly built
    /// instance before it's inserted — otherwise a restart would silently leave a
    /// plugin marked enabled with no actual background work running.
    pub async fn register(
        &self,
        builder: Arc<dyn PluginBuilder>,
        initial_settings: Option<Value>,
        enabled: bool,
    ) -> Result<(), PluginError> {
        let plugin_name = builder.plugin_name().to_string();
        let plugin_subname = builder.plugin_subname().to_string();

        let (initial_settings, enabled) = match self.store.get(&plugin_name, &plugin_subname).await? {
            Some(persisted) => (Some(persisted.settings), persisted.enabled),
            None => (initial_settings, enabled),
        };

        if enabled && initial_settings.is_none() {
            return Err(PluginError::FailedUnknown(
                "cannot register a plugin as enabled with no settings".to_string(),
            ));
        }

        let (plugin, router) = match initial_settings.clone() {
            Some(settings) => {
                let plugin = builder.build(settings).await?;
                let router = plugin.api_router();
                (Some(plugin), Some(router))
            }
            None => (None, None),
        };

        if enabled {
            if let Some(plugin) = &plugin {
                plugin.on_enabled().await?;
            }
        }

        // Only persisted when there's actually something to persist — a plugin
        // registered with no settings at all (from either source) has nothing to write.
        if let Some(settings) = initial_settings {
            self.store.set(&plugin_name, &plugin_subname, settings, enabled).await?;
        }

        let key = Self::key(&plugin_name, &plugin_subname);
        self.entries.write().await.insert(key, PluginEntry { builder, plugin, router, enabled });
        Ok(())
    }

    /// Registers every builder in `builders` under its own key, in order — the way
    /// `main.rs` is meant to register the whole known plugin list at startup instead of
    /// calling `register` once per builder by hand. No `initial_settings`/`enabled` to
    /// pass per builder: `register` already prefers whatever `PluginSettingsStore` has
    /// persisted for that key over anything the caller supplies, so a plugin that was
    /// configured and enabled before a restart comes back that way regardless — this
    /// just needs to name every builder that exists, not know any of their defaults.
    /// Stops at the first failure (an `Err` here means a real bug — e.g. two builders
    /// reporting the same key — not a per-plugin runtime condition to shrug off).
    pub async fn register_many(&self, builders: Vec<Arc<dyn PluginBuilder>>) -> Result<(), PluginError> {
        for builder in builders {
            self.register(builder, None, false).await?;
        }
        Ok(())
    }

    /// Builds (or rebuilds) a registered plugin's instance from settings, via the same
    /// builder it was registered with, and swaps it in — used both the first time
    /// settings are supplied for a plugin registered with `None` and for every
    /// settings change after that. See `PluginBuilder`'s doc comment for why this is a
    /// rebuild rather than an in-place update. Doesn't change `enabled` — a plugin
    /// getting its first settings still needs a separate `set_enabled(true)` call.
    /// Persists the new settings (alongside the *existing* `enabled` value, unchanged)
    /// so they survive a restart. If the plugin is currently enabled, the *old*
    /// instance's `on_disabled` runs before the rebuild and the *new* instance's
    /// `on_enabled` runs right after — a plain swap would otherwise orphan the old
    /// instance's background work (its `Arc` gets dropped, but a spawned `tokio` task
    /// doesn't stop just because nothing holds its `JoinHandle` anymore) while the new
    /// instance never actually started.
    pub async fn update_settings(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        settings: Value,
    ) -> Result<(), PluginError> {
        let mut entries = self.entries.write().await;
        let entry = entries.get_mut(&Self::key(plugin_name, plugin_subname)).ok_or_else(|| {
            PluginError::NotFound(format!("no such plugin {plugin_name}/{plugin_subname}"))
        })?;

        if entry.enabled {
            if let Some(old_plugin) = &entry.plugin {
                old_plugin.on_disabled().await?;
            }
        }

        let plugin = entry.builder.build(settings.clone()).await?;
        entry.router = Some(plugin.api_router());

        if entry.enabled {
            plugin.on_enabled().await?;
        }
        entry.plugin = Some(plugin);

        self.store.set(plugin_name, plugin_subname, settings, entry.enabled).await?;
        Ok(())
    }

    pub async fn get(&self, plugin_name: &str, plugin_subname: &str) -> Option<Arc<dyn Plugin>> {
        self.entries
            .read()
            .await
            .get(&Self::key(plugin_name, plugin_subname))
            .and_then(|entry| entry.plugin.clone())
    }

    /// The current instance's already-built router — see `PluginEntry::router` for why
    /// this is a cached clone rather than a fresh `plugin.api_router()` call. `None` if
    /// the plugin is unregistered, has no settings yet, or is currently disabled — a
    /// disabled plugin's routes are meant to be unreachable, so this is also the
    /// enabled-gate `proxy_for` relies on, not just a cache lookup.
    async fn get_router(&self, plugin_name: &str, plugin_subname: &str) -> Option<Router> {
        self.entries
            .read()
            .await
            .get(&Self::key(plugin_name, plugin_subname))
            .filter(|entry| entry.enabled)
            .and_then(|entry| entry.router.clone())
    }

    pub async fn builder(&self, plugin_name: &str, plugin_subname: &str) -> Option<Arc<dyn PluginBuilder>> {
        self.entries
            .read()
            .await
            .get(&Self::key(plugin_name, plugin_subname))
            .map(|entry| entry.builder.clone())
    }

    pub async fn is_enabled(&self, plugin_name: &str, plugin_subname: &str) -> bool {
        self.entries
            .read()
            .await
            .get(&Self::key(plugin_name, plugin_subname))
            .is_some_and(|entry| entry.enabled)
    }

    /// Errors if the plugin isn't registered, or if `enabled: true` is requested for
    /// one with no settings yet (`plugin` still `None` — see `PluginEntry`). A no-op
    /// (no hook call, no write) if `enabled` already matches the current state — this
    /// is what keeps a repeated `set_enabled(true)` from calling `on_enabled` again on
    /// top of an already-running instance (`MessagingPlugin::on_enabled` in particular
    /// would silently replace its `RunningLoops` without aborting the old one first).
    /// Otherwise runs the matching hook (`on_enabled`/`on_disabled`) on the live
    /// instance *before* flipping the flag or persisting — a hook failure means the
    /// requested state never actually took effect, so nothing here should claim it did.
    pub async fn set_enabled(&self, plugin_name: &str, plugin_subname: &str, enabled: bool) -> Result<(), PluginError> {
        let mut entries = self.entries.write().await;
        let entry = entries.get_mut(&Self::key(plugin_name, plugin_subname)).ok_or_else(|| {
            PluginError::NotFound(format!("no such plugin {plugin_name}/{plugin_subname}"))
        })?;
        if enabled && entry.plugin.is_none() {
            return Err(PluginError::InvalidState(format!(
                "{plugin_name}/{plugin_subname} has no settings configured yet"
            )));
        }

        if enabled == entry.enabled {
            return Ok(());
        }

        if let Some(plugin) = &entry.plugin {
            if enabled {
                plugin.on_enabled().await?;
            } else {
                plugin.on_disabled().await?;
            }
        }

        entry.enabled = enabled;

        if let Some(plugin) = &entry.plugin {
            self.store.set(plugin_name, plugin_subname, plugin.settings_value(), enabled).await?;
        }

        Ok(())
    }

    /// Every registered plugin, enabled and disabled alike — the `GET /api/plugins`
    /// listing. `settings` is `None` for a plugin that's known (has a builder, could be
    /// configured) but hasn't been given settings yet, same meaning as `PluginEntry`'s
    /// own `plugin: Option<_>`.
    pub async fn list(&self) -> Vec<PluginInfo> {
        self.entries
            .read()
            .await
            .iter()
            .map(|(key, entry)| PluginInfo {
                plugin_name: key.0.clone(),
                plugin_subname: key.1.clone(),
                enabled: entry.enabled,
                settings: entry.plugin.as_ref().map(|plugin| plugin.settings_value()),
            })
            .collect()
    }

    /// Builds one `Router` nesting a stable proxy for every plugin key registered *so
    /// far* — call once at startup, after every `register()` call, and mount the result
    /// under `/api/plugins`. A plugin key registered later than this call never gets a
    /// route (axum's tree is fixed once handed to the server — see `proxy_for`'s own
    /// doc comment for the same constraint one level down); this project always
    /// registers every known plugin at startup before serving, same as the tool list,
    /// so that's not a real limitation in practice.
    pub async fn router(self: &Arc<Self>) -> Router {
        let keys: Vec<PluginKey> = self.entries.read().await.keys().cloned().collect();
        let mut router = Router::new();
        for (plugin_name, plugin_subname) in keys {
            let path = format!("/{plugin_name}/{plugin_subname}");
            router = router.nest(&path, self.proxy_for(plugin_name, plugin_subname));
        }
        router
    }

    /// A stable stand-in for one plugin's `api_router()` — mount *this* under the
    /// plugin's path once, forever, instead of the plugin's own router directly.
    ///
    /// axum's route tree is fixed the moment the server starts serving: nesting a
    /// specific `Router` value bakes that exact value in, so a later
    /// `update_settings()` rebuild (a brand new `Arc<dyn Plugin>`, per
    /// `PluginBuilder`'s doc comment) would be invisible to already-mounted routes if
    /// they were wired to the old instance directly. This proxy sidesteps that by never
    /// closing over a specific instance at all — its handler looks up whatever's
    /// *currently* in the registry fresh, on every single request, and forwards into
    /// that instance's own `api_router()` (via `Router: tower::Service`, `.oneshot()`
    /// runs one request through it). So the mounted route never changes, but the plugin
    /// behind it can be swapped any number of times.
    ///
    /// A disabled-but-configured plugin's routes 404 through here too — `get_router`
    /// itself is the enabled gate (see its own doc comment), so this handler doesn't
    /// need a separate check.
    ///
    /// Looks up the *cached* router (see `PluginEntry::router`/`get_router`), not a
    /// fresh `plugin.api_router()` — the router only needs rebuilding when the plugin
    /// does, which already happens in `register`/`update_settings`, not once per
    /// request.
    fn proxy_for(self: &Arc<Self>, plugin_name: String, plugin_subname: String) -> Router {
        let registry = self.clone();
        Router::new().fallback(move |req: Request| {
            let registry = registry.clone();
            let plugin_name = plugin_name.clone();
            let plugin_subname = plugin_subname.clone();
            async move {
                let Some(router) = registry.get_router(&plugin_name, &plugin_subname).await else {
                    return StatusCode::NOT_FOUND.into_response();
                };
                router.oneshot(req).await.unwrap().into_response()
            }
        })
    }

    fn key(plugin_name: &str, plugin_subname: &str) -> PluginKey {
        (plugin_name.to_string(), plugin_subname.to_string())
    }
}
