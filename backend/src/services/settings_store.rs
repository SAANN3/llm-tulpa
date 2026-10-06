mod entities;

use std::sync::Arc;

use axum::http::StatusCode;
use entities::settings;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, SqlErr};

use crate::services::error::ErrorService;
use crate::services::model_store::{ModelRef, ModelStore, ModelStoreErrors};

/// The provider reported for a user who hasn't picked a model yet.
const DEFAULT_PROVIDER: &str = "llama-cpp";

/// The largest custom system prompt accepted — it's sent to the model on every single turn,
/// so its size is a recurring context cost, and an unbounded one would eat the budget whole.
const MAX_SYSTEM_PROMPT_CHARS: usize = 100_000;
/// The most a user can set the turn step limit to: past this it is a limit in name only.
const MAX_TURN_STEPS_LIMIT: i32 = 10_000;

/// Owns per-user settings (the `user_settings` table, 1:1 with `users`). A user's row is
/// created empty the first time it's needed (see `row`) and filled in over the setup wizard. The active model lives in the `llm_models` table and
/// is referenced by id — its name and provider are resolved through `ModelStore`.
pub struct SettingsStore {
    db: DatabaseConnection,
    models: Arc<ModelStore>,
}

impl SettingsStore {
    pub fn new(db: DatabaseConnection, models: Arc<ModelStore>) -> Self {
        Self { db, models }
    }

    /// The user's settings row, created empty on first use — `UserStore` never touches this
    /// table, so a fresh user simply has no row until something here asks for it. Errs
    /// `NotFound` if the user doesn't exist (the foreign key refuses the insert).
    async fn row(&self, user_id: i64) -> Result<settings::Model, SettingsStoreErrors> {
        if let Some(row) = settings::Entity::find_by_id(user_id).one(&self.db).await? {
            return Ok(row);
        }

        settings::Entity::insert(settings::ActiveModel {
            user_id: Set(user_id),
            ..Default::default()
        })
        .on_conflict_do_nothing()
        .exec_without_returning(&self.db)
        .await
        .map_err(|e| match e.sql_err() {
            Some(SqlErr::ForeignKeyConstraintViolation(_)) => SettingsStoreErrors::NotFound,
            _ => SettingsStoreErrors::QueryFailed(e),
        })?;

        settings::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .ok_or(SettingsStoreErrors::NotFound)
    }

    /// A user's settings — mostly-null fields for a user who hasn't set any yet. Errs
    /// `NotFound` only if the user is gone.
    pub async fn settings(&self, user_id: i64) -> Result<Settings, SettingsStoreErrors> {
        let row = self.row(user_id).await?;

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
            auto_confirm: row.auto_confirm,
            trim_old_thinking: row.trim_old_thinking,
            max_turn_steps: row.max_turn_steps,
            has_hf_token: row.hf_token.is_some(),
            llm_provider: active
                .as_ref()
                .map(|m| m.provider.clone())
                .unwrap_or_else(|| DEFAULT_PROVIDER.to_string()),
            active_model: active.map(|m| m.name),
            launch_profile_id: row.active_profile_id,
        })
    }

    /// The model one-shot prompts (greeting, chat naming, ...) run against for this user: their
    /// active model, else the first registered one, with the provider it belongs to. Errs
    /// `NoModel` when none has been picked yet.
    pub async fn effective_model(&self, user_id: i64) -> Result<ModelRef, SettingsStoreErrors> {
        Ok(self.models.resolve_default(user_id).await?)
    }

    /// Whether the user has completed the minimum settings the app needs (a name and a
    /// timezone) — used to gate the "settings configured" frontend check.
    pub async fn is_configured(&self, user_id: i64) -> Result<bool, SettingsStoreErrors> {
        let row = self.row(user_id).await?;
        Ok(row.name.is_some() && row.timezone.is_some())
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

        let existing = self.row(user_id).await?;

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
        if let Some(auto_confirm) = update.auto_confirm {
            model.auto_confirm = Set(auto_confirm);
        }
        if let Some(trim) = update.trim_old_thinking {
            model.trim_old_thinking = Set(trim);
        }
        if let Some(steps) = update.max_turn_steps {
            if !(0..=MAX_TURN_STEPS_LIMIT).contains(&steps) {
                return Err(SettingsStoreErrors::InvalidTurnSteps(steps));
            }
            // 0 removes the limit
            model.max_turn_steps = Set(Some(steps).filter(|s| *s > 0));
        }
        if let Some(token) = update.hf_token {
            // An empty token clears it
            model.hf_token = Set(Some(token.trim().to_string()).filter(|t| !t.is_empty()));
        }
        if let Some(id) = active_model_id {
            model.active_model_id = Set(Some(id));
            // The profile belongs to the model it was chosen for; a new model starts on its first
            model.active_profile_id = Set(update.launch_profile_id);
        } else if let Some(profile_id) = update.launch_profile_id {
            model.active_profile_id = Set(Some(profile_id));
        }

        model.update(&self.db).await?;
        Ok(())
    }

    /// The user's Hugging Face token, when they have set one.
    pub async fn hf_token(&self, user_id: i64) -> Result<Option<String>, SettingsStoreErrors> {
        Ok(self.row(user_id).await?.hf_token)
    }

    /// Whether the user has tool-permission prompts approved automatically.
    pub async fn auto_confirm(&self, user_id: i64) -> Result<bool, SettingsStoreErrors> {
        Ok(self.row(user_id).await?.auto_confirm)
    }

    /// Whether old thinking traces are shortened in this user's long chats.
    pub async fn trim_old_thinking(&self, user_id: i64) -> Result<bool, SettingsStoreErrors> {
        Ok(self.row(user_id).await?.trim_old_thinking)
    }

    /// How many model calls one of the user's turns may make, `None` for no limit.
    pub async fn max_turn_steps(&self, user_id: i64) -> Result<Option<u32>, SettingsStoreErrors> {
        Ok(self.row(user_id).await?.max_turn_steps.map(|s| s as u32))
    }

    /// The user's custom system prompt — `None` while the built-in default applies. Like the
    /// rest of the store, creates the user's empty row on first use.
    pub async fn system_prompt(&self, user_id: i64) -> Result<Option<String>, SettingsStoreErrors> {
        Ok(self.row(user_id).await?.system_prompt)
    }

    /// Sets (or clears, for `None`) the user's custom system prompt. A whitespace-only
    /// prompt is treated as no prompt at all, and one past `MAX_SYSTEM_PROMPT_CHARS` is
    /// refused rather than stored — it's prepended to the model's prompt on every turn, so
    /// its size is a per-turn context cost.
    pub async fn set_system_prompt(&self, user_id: i64, prompt: Option<String>) -> Result<(), SettingsStoreErrors> {
        let prompt = prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        if let Some(p) = &prompt {
            if p.chars().count() > MAX_SYSTEM_PROMPT_CHARS {
                return Err(SettingsStoreErrors::SystemPromptTooLarge(MAX_SYSTEM_PROMPT_CHARS));
            }
        }

        let existing = self.row(user_id).await?;
        let mut model: settings::ActiveModel = existing.into();
        model.system_prompt = Set(prompt);
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
    /// Tool-permission prompts are approved automatically instead of waiting for the user.
    pub auto_confirm: bool,
    /// In a long chat, thinking traces from earlier turns are shortened to their tail when the
    /// context nears its limit, and the newest traces are replayed in more of their length.
    pub trim_old_thinking: bool,
    /// How many model calls one turn may make before it is stopped and the model is asked to wrap up;
    /// `None` for no limit.
    pub max_turn_steps: Option<i32>,
    /// Whether a Hugging Face token is set (the token itself is never sent back).
    pub has_hf_token: bool,
    /// The provider of the active model (`ollama` until one is picked).
    pub llm_provider: String,
    pub active_model: Option<String>,
    /// The launch profile of that model new chats start on; `None` means its first profile.
    pub launch_profile_id: Option<i64>,
}

/// A partial settings update — every `None` field is left unchanged.
#[derive(serde::Deserialize, utoipa::ToSchema, Default)]
pub struct SettingsUpdate {
    pub name: Option<String>,
    pub timezone: Option<i32>,
    pub notifications_enabled: Option<bool>,
    pub theme: Option<String>,
    pub language: Option<String>,
    pub auto_confirm: Option<bool>,
    pub trim_old_thinking: Option<bool>,
    /// The turn step limit, 0 for none
    pub max_turn_steps: Option<i32>,
    /// A Hugging Face access token; empty clears it
    pub hf_token: Option<String>,
    pub llm_provider: Option<String>,
    pub active_model: Option<String>,
    /// The launch profile of `active_model` new chats start on. A new `active_model` without one clears
    /// the choice, since a profile belongs to the model it was made for.
    pub launch_profile_id: Option<i64>,
}

/// The user's custom system prompt alongside the built-in default, for the settings page's
/// edit pane — `custom` is `None` while the default applies.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SystemPromptOut {
    pub custom: Option<String>,
    pub default: String,
}

/// A custom-system-prompt update: `Some` text replaces the prompt, `None` (or a `null`
/// body field) resets it to the built-in default. Unlike `SettingsUpdate` there is no
/// "leave unchanged" state — the body always states the whole new value.
#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct SystemPromptUpdate {
    pub prompt: Option<String>,
}

#[derive(Debug)]
pub enum SettingsStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    InvalidTimezone(i32),
    Model(ModelStoreErrors),
    /// A custom system prompt past `MAX_SYSTEM_PROMPT_CHARS`.
    SystemPromptTooLarge(usize),
    /// A turn step limit outside `0..=MAX_TURN_STEPS_LIMIT`.
    InvalidTurnSteps(i32),
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
            SettingsStoreErrors::InvalidTurnSteps(steps) => ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("the turn step limit {steps} is out of range (0 for none, at most {MAX_TURN_STEPS_LIMIT})"),
            ),
            SettingsStoreErrors::SystemPromptTooLarge(max) => ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("a custom system prompt may be at most {max} characters"),
            ),
        }
    }
}
