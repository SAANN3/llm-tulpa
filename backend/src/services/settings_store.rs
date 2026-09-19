mod entities;

use std::sync::Arc;

use axum::http::StatusCode;
use entities::settings;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection};

use crate::services::error::ErrorService;
use crate::services::model_store::{ModelStore, ModelStoreErrors};

/// The provider reported for a user who hasn't picked a model yet.
const DEFAULT_PROVIDER: &str = "ollama";

/// Owns per-user settings (the `user_settings` table, 1:1 with `users`). The row is
/// created empty alongside its user (see `UserStore::create_user`) and filled in over the
/// setup wizard, so every operation here is an update of an existing row keyed by
/// `user_id`, never an insert. The active model lives in the `llm_models` table and is
/// referenced by id — its name and provider are resolved through `ModelStore`.
pub struct SettingsStore {
    db: DatabaseConnection,
    models: Arc<ModelStore>,
}

impl SettingsStore {
    pub fn new(db: DatabaseConnection, models: Arc<ModelStore>) -> Self {
        Self { db, models }
    }

    /// A user's settings. Errs `NotFound` only if the user (hence its settings row) is
    /// gone — a fresh user's row exists but with mostly-null fields.
    pub async fn settings(&self, user_id: i64) -> Result<Settings, SettingsStoreErrors> {
        let row = settings::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or(SettingsStoreErrors::NotFound)?;

        let active = match row.active_model_id {
            Some(id) => self.models.get_many(&[id]).await?.remove(&id),
            None => None,
        };

        Ok(Settings {
            name: row.name,
            timezone: row.timezone.map(|t| t as i32),
            notifications_enabled: row.notifications_enabled,
            theme: row.theme,
            language: row.language,
            llm_provider: active
                .as_ref()
                .map(|m| m.provider.clone())
                .unwrap_or_else(|| DEFAULT_PROVIDER.to_string()),
            active_model: active.map(|m| m.name),
        })
    }

    /// The model name one-shot prompts (greeting, chat naming, ...) run against for this
    /// user: their active model, else the first registered one. Errs `NoModel` when none
    /// has been picked yet.
    pub async fn effective_model(&self, user_id: i64) -> Result<String, SettingsStoreErrors> {
        Ok(self.models.resolve_default(user_id).await?.name)
    }

    /// Whether the user has completed the minimum settings the app needs (a name and a
    /// timezone) — used to gate the "settings configured" frontend check.
    pub async fn is_configured(&self, user_id: i64) -> Result<bool, SettingsStoreErrors> {
        let row = settings::Entity::find_by_id(user_id).one(&self.db).await?;
        Ok(row.is_some_and(|r| r.name.is_some() && r.timezone.is_some()))
    }

    /// Applies a partial update — only the `Some` fields are written, the rest left as-is.
    /// Supports both the wizard's incremental per-step saves and a full settings save.
    /// `active_model` registers the model (under `llm_provider`, else its current provider,
    /// else the default one) and points the user at it; `llm_provider` alone only validates
    /// the provider, since a provider is persisted through the model that belongs to it.
    pub async fn update(&self, user_id: i64, update: SettingsUpdate) -> Result<(), SettingsStoreErrors> {
        if let Some(tz) = update.timezone {
            if !(-12..=14).contains(&tz) {
                return Err(SettingsStoreErrors::InvalidTimezone(tz));
            }
        }

        let existing = settings::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or(SettingsStoreErrors::NotFound)?;

        let mut active_model_id = None;
        match (&update.active_model, &update.llm_provider) {
            (Some(name), provider) => {
                let provider = match provider {
                    Some(provider) => provider.clone(),
                    None => self.settings(user_id).await?.llm_provider,
                };
                active_model_id = Some(self.models.ensure(&provider, name).await?.id);
            }
            (None, Some(provider)) => {
                if !self.models.providers().await?.iter().any(|p| p == provider) {
                    return Err(ModelStoreErrors::UnknownProvider(provider.clone()).into());
                }
            }
            (None, None) => {}
        }

        let mut model: settings::ActiveModel = existing.into();
        if let Some(name) = update.name {
            model.name = Set(Some(name));
        }
        if let Some(tz) = update.timezone {
            model.timezone = Set(Some(tz as i16));
        }
        if let Some(n) = update.notifications_enabled {
            model.notifications_enabled = Set(n);
        }
        if let Some(theme) = update.theme {
            model.theme = Set(Some(theme));
        }
        if let Some(language) = update.language {
            model.language = Set(language);
        }
        if let Some(id) = active_model_id {
            model.active_model_id = Set(Some(id));
        }

        model.update(&self.db).await?;
        Ok(())
    }
}

#[derive(serde::Serialize, utoipa::ToSchema, Clone)]
pub struct Settings {
    pub name: Option<String>,
    pub timezone: Option<i32>,
    pub notifications_enabled: bool,
    pub theme: Option<String>,
    pub language: String,
    /// The provider of the active model (`ollama` until one is picked).
    pub llm_provider: String,
    pub active_model: Option<String>,
}

/// A partial settings update — every `None` field is left unchanged.
#[derive(serde::Deserialize, utoipa::ToSchema, Default)]
pub struct SettingsUpdate {
    pub name: Option<String>,
    pub timezone: Option<i32>,
    pub notifications_enabled: Option<bool>,
    pub theme: Option<String>,
    pub language: Option<String>,
    pub llm_provider: Option<String>,
    pub active_model: Option<String>,
}

#[derive(Debug)]
pub enum SettingsStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    InvalidTimezone(i32),
    Model(ModelStoreErrors),
}

impl From<DbErr> for SettingsStoreErrors {
    fn from(err: DbErr) -> Self {
        SettingsStoreErrors::QueryFailed(err)
    }
}

impl From<ModelStoreErrors> for SettingsStoreErrors {
    fn from(err: ModelStoreErrors) -> Self {
        SettingsStoreErrors::Model(err)
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
            SettingsStoreErrors::Model(e) => e.into(),
        }
    }
}
