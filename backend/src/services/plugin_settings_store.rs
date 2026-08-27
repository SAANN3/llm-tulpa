mod entities;
mod migrate;

use entities::plugin_settings;
use migrate::migrate;
use sea_orm::{
    ActiveValue::Set, Database, DatabaseConnection, DbBackend, Statement, prelude::*,
};

use crate::services::error::ErrorService;

/// What's persisted for one plugin instance — settings plus whether it was enabled,
/// so a restart doesn't silently come back with a plugin the user turned on now
/// looking off. `None` (not this struct) is how "never configured yet" is expressed —
/// see `get` below.
pub struct PersistedPlugin {
    pub settings: serde_json::Value,
    pub enabled: bool,
}

/// Owns persistence for plugin settings — the durable counterpart to
/// `PluginRegistry`'s in-memory `PluginEntry` (see `plugins/registry.rs`), which holds
/// the live instance but forgets everything on restart. Keyed by `(plugin_name,
/// plugin_subname)`, same as the registry itself. Single responsibility, kept separate
/// from `SettingsStore` (one global user-settings row, not per-plugin) and
/// `ChatStore`/`PermissionStore` (chat content and per-chat tool grants, not plugin
/// config) — same isolated-SeaORM-entity shape as the other stores.
pub struct PluginSettingsStore {
    db: DatabaseConnection,
}

impl PluginSettingsStore {
    /// Same create-database-if-missing-then-migrate bootstrap as the other stores.
    pub async fn new(base_url: &str, db_name: &str) -> Self {
        let admin_url = format!("{base_url}/postgres");

        let admin_db = Database::connect(&admin_url).await.unwrap_or_else(|e| {
            panic!("failed to connect to postgres to check/create database '{db_name}': {e}")
        });

        let exists = admin_db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT 1 FROM pg_database WHERE datname = $1",
                [db_name.into()],
            ))
            .await
            .unwrap_or_else(|e| panic!("failed to check whether database '{db_name}' exists: {e}"))
            .is_some();

        if !exists {
            admin_db
                .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
                .await
                .unwrap_or_else(|e| panic!("failed to create database '{db_name}': {e}"));
        }

        admin_db
            .close()
            .await
            .unwrap_or_else(|e| panic!("failed to close bootstrap connection: {e}"));

        let target_url = format!("{base_url}/{db_name}");

        let db = Database::connect(&target_url)
            .await
            .unwrap_or_else(|e| panic!("failed to connect to database '{db_name}': {e}"));

        migrate(&db)
            .await
            .unwrap_or_else(|e| panic!("failed to run migrations: {e}"));

        Self { db }
    }

    /// `None` if nothing's ever been persisted for this plugin — a normal state for a
    /// freshly-registered plugin (or one that's never had settings entered), not an
    /// error.
    pub async fn get(&self, plugin_name: &str, plugin_subname: &str) -> Result<Option<PersistedPlugin>, PluginSettingsStoreErrors> {
        let row = plugin_settings::Entity::find()
            .filter(plugin_settings::Column::PluginName.eq(plugin_name))
            .filter(plugin_settings::Column::PluginSubname.eq(plugin_subname))
            .one(&self.db)
            .await?;

        Ok(row.map(|row| PersistedPlugin { settings: row.settings, enabled: row.enabled }))
    }

    /// Inserts or replaces the persisted settings/enabled state for this plugin —
    /// `ON CONFLICT` on the `(plugin_name, plugin_subname)` unique constraint from
    /// `migrate`. Settings and `enabled` are always written together rather than as
    /// separate calls: an `enabled` flag with no corresponding settings wouldn't mean
    /// anything (same invariant `PluginRegistry::set_enabled` enforces in memory).
    pub async fn set(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        settings: serde_json::Value,
        enabled: bool,
    ) -> Result<(), PluginSettingsStoreErrors> {
        let existing = plugin_settings::Entity::find()
            .filter(plugin_settings::Column::PluginName.eq(plugin_name))
            .filter(plugin_settings::Column::PluginSubname.eq(plugin_subname))
            .one(&self.db)
            .await?;

        let model = plugin_settings::ActiveModel {
            id: existing.as_ref().map_or(sea_orm::ActiveValue::NotSet, |row| Set(row.id)),
            plugin_name: Set(plugin_name.to_string()),
            plugin_subname: Set(plugin_subname.to_string()),
            settings: Set(settings),
            enabled: Set(enabled),
        };

        if existing.is_some() {
            model.update(&self.db).await?;
        } else {
            model.insert(&self.db).await?;
        }

        Ok(())
    }
}

/// Mirrors `PermissionStoreErrors`'s shape, minus `NotFound` — "nothing persisted yet"
/// is expressed as `Ok(None)` from `get` instead, since it's an expected state, not a
/// failure.
#[derive(Debug)]
pub enum PluginSettingsStoreErrors {
    QueryFailed(DbErr),
}

impl From<DbErr> for PluginSettingsStoreErrors {
    fn from(err: DbErr) -> Self {
        PluginSettingsStoreErrors::QueryFailed(err)
    }
}

impl From<PluginSettingsStoreErrors> for ErrorService {
    fn from(err: PluginSettingsStoreErrors) -> Self {
        match err {
            PluginSettingsStoreErrors::QueryFailed(e) => {
                tracing::error!("plugin settings store query failed: {e}");
                ErrorService::internal("database query failed")
            }
        }
    }
}
