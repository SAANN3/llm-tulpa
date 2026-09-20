mod entities;

use entities::plugin_settings;
use std::sync::Arc;

use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection};

use crate::services::error::ErrorService;
use crate::services::user_store::{UserStore, UserStoreErrors};

/// What's persisted for one plugin instance — settings plus whether it was enabled.
pub struct PersistedPlugin {
    pub settings: serde_json::Value,
    pub enabled: bool,
}

/// Owns persistence for plugin settings (table `user_plugins`). Plugins are owned by the
/// owner user, resolved internally so callers (the registry, plugin routes) don't have to
/// thread a `user_id` — a deliberate simplification while per-user plugins are deferred.
pub struct PluginSettingsStore {
    db: DatabaseConnection,
    /// Only for `owner_id`: the `users` table is `UserStore`'s, not this store's.
    users: Arc<UserStore>,
}

impl PluginSettingsStore {
    pub fn new(db: DatabaseConnection, users: Arc<UserStore>) -> Self {
        Self { db, users }
    }

    /// The owner user's id, or `None` if setup hasn't created one yet.
    async fn owner_id(&self) -> Result<Option<i64>, PluginSettingsStoreErrors> {
        Ok(self.users.owner_id().await?)
    }

    /// `None` if nothing's persisted for this plugin (or there's no owner yet).
    pub async fn get(&self, plugin_name: &str, plugin_subname: &str) -> Result<Option<PersistedPlugin>, PluginSettingsStoreErrors> {
        let Some(owner) = self.owner_id().await? else {
            return Ok(None);
        };

        let row = plugin_settings::Entity::find()
            .filter(plugin_settings::Column::UserId.eq(owner))
            .filter(plugin_settings::Column::PluginName.eq(plugin_name))
            .filter(plugin_settings::Column::PluginSubname.eq(plugin_subname))
            .one(&self.db)
            .await?;

        Ok(row.map(|row| PersistedPlugin { settings: row.settings, enabled: row.enabled }))
    }

    /// Inserts or replaces the persisted settings/enabled state for this plugin under the
    /// owner user. Errs if there's no owner yet.
    pub async fn set(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        settings: serde_json::Value,
        enabled: bool,
    ) -> Result<(), PluginSettingsStoreErrors> {
        let owner = self.owner_id().await?.ok_or(PluginSettingsStoreErrors::NoOwner)?;

        let existing = plugin_settings::Entity::find()
            .filter(plugin_settings::Column::UserId.eq(owner))
            .filter(plugin_settings::Column::PluginName.eq(plugin_name))
            .filter(plugin_settings::Column::PluginSubname.eq(plugin_subname))
            .one(&self.db)
            .await?;

        let model = plugin_settings::ActiveModel {
            id: existing.as_ref().map_or(sea_orm::ActiveValue::NotSet, |row| Set(row.id)),
            user_id: Set(owner),
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

#[derive(Debug)]
pub enum PluginSettingsStoreErrors {
    QueryFailed(DbErr),
    User(UserStoreErrors),
    NoOwner,
}

impl From<UserStoreErrors> for PluginSettingsStoreErrors {
    fn from(err: UserStoreErrors) -> Self {
        PluginSettingsStoreErrors::User(err)
    }
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
            PluginSettingsStoreErrors::User(e) => e.into(),
            PluginSettingsStoreErrors::NoOwner => {
                ErrorService::internal("no owner user configured yet")
            }
        }
    }
}
