mod entities;
mod migrate;

use axum::http::StatusCode;
use entities::settings;
use migrate::migrate;
use sea_orm::{prelude::*, ActiveValue::Set, Database, DatabaseConnection, DbBackend, Statement};

use crate::services::error::ErrorService;

/// Postgres row id every settings row lives at — see `entities::settings::Model` for why
/// there's only ever one.
const SETTINGS_ID: i16 = 1;

/// Owns the single persisted-settings row (currently: display name + UTC offset). Same
/// isolated-SeaORM-entity shape as `ChatStore` — the `settings` entity is private to this
/// module, callers only ever see the plain `Settings` struct below. Persisted (not just
/// in-memory) specifically because `UserCacheService`'s background refresh loop needs a
/// timezone available before any frontend request ever arrives, including on a fresh
/// restart.
pub struct SettingsStore {
    db: DatabaseConnection,
}

impl SettingsStore {
    /// Same create-database-if-missing-then-migrate bootstrap as `ChatStore::new` —
    /// takes the same `base_url`/`db_name` the rest of the app already connects with, so
    /// in practice the database already exists by the time this runs. Kept structurally
    /// identical anyway so `SettingsStore` doesn't implicitly depend on construction
    /// order elsewhere in `main.rs`. Panics on any failure, for the same reason
    /// `ChatStore::new` does: there's no reasonable way to run this app without a working
    /// database.
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

    /// Cheap existence check, used by `UserCacheService` to decide whether it's safe to
    /// start its background loop (it can't, without a timezone) — separate from
    /// `settings()` so callers that only need a yes/no don't have to match on
    /// `SettingsStoreErrors::NotFound`.
    pub async fn has_settings(&self) -> Result<bool, SettingsStoreErrors> {
        let exists = settings::Entity::find_by_id(SETTINGS_ID)
            .one(&self.db)
            .await?
            .is_some();

        Ok(exists)
    }

    /// The persisted settings. Errs with `NotFound` if nothing has been saved yet —
    /// there's no default timezone that would ever be more correct than "not configured
    /// yet."
    pub async fn settings(&self) -> Result<Settings, SettingsStoreErrors> {
        let row = settings::Entity::find_by_id(SETTINGS_ID)
            .one(&self.db)
            .await?
            .ok_or(SettingsStoreErrors::NotFound)?;

        Ok(Settings {
            name: row.name,
            timezone: row.timezone as i32,
            notifications_enabled: row.notifications_enabled,
        })
    }

    /// Upserts the single settings row. `timezone` is validated as a real UTC offset
    /// (`-12..=14`) since a garbage value here would silently corrupt every future greet
    /// generation, not just fail loudly once.
    pub async fn set_settings(&self, settings: Settings) -> Result<(), SettingsStoreErrors> {
        if !(-12..=14).contains(&settings.timezone) {
            return Err(SettingsStoreErrors::InvalidTimezone(settings.timezone));
        }

        let existing = settings::Entity::find_by_id(SETTINGS_ID).one(&self.db).await?;

        let model = settings::ActiveModel {
            id: Set(SETTINGS_ID),
            name: Set(settings.name),
            timezone: Set(settings.timezone as i16),
            notifications_enabled: Set(settings.notifications_enabled),
        };

        if existing.is_some() {
            model.update(&self.db).await?;
        } else {
            model.insert(&self.db).await?;
        }

        Ok(())
    }

    /// Deletes the persisted settings row, if any — not an error if there wasn't one.
    /// Mainly a dev/testing hook for exercising the "settings not configured yet" UI
    /// path without going through Postgres directly.
    pub async fn delete_settings(&self) -> Result<(), SettingsStoreErrors> {
        settings::Entity::delete_by_id(SETTINGS_ID).exec(&self.db).await?;

        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema, Clone)]
pub struct Settings {
    pub name: String,
    /// UTC offset in whole hours (e.g. `-5`, `9`), not an IANA timezone name.
    pub timezone: i32,
    /// Whether the frontend should ask the browser to show a notification when an
    /// assistant reply finishes. Purely a stored preference — the actual browser
    /// permission grant is a separate, per-browser thing the frontend has to request on
    /// its own.
    pub notifications_enabled: bool,
}

/// Mirrors `ChatStoreErrors`'s shape — one generic `QueryFailed` wrapping any SeaORM
/// failure, plus the domain-specific cases that map to a different HTTP status than 500.
#[derive(Debug)]
pub enum SettingsStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    InvalidTimezone(i32),
}

impl From<DbErr> for SettingsStoreErrors {
    fn from(err: DbErr) -> Self {
        SettingsStoreErrors::QueryFailed(err)
    }
}

impl From<SettingsStoreErrors> for ErrorService {
    fn from(err: SettingsStoreErrors) -> Self {
        match err {
            SettingsStoreErrors::QueryFailed(e) => {
                tracing::error!("settings store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            SettingsStoreErrors::NotFound => {
                ErrorService::new(StatusCode::NOT_FOUND, "settings have not been configured yet")
            }
            SettingsStoreErrors::InvalidTimezone(tz) => ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("timezone offset {tz} is out of range (-12..=14)"),
            ),
        }
    }
}
