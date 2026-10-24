//! The record of the external plugins the backend has seen: what each is, where it came from and whether it is running, so
//! that a plugin that could not start (a failed check, a broken manifest) is still there to be shown with its reason, and a
//! plugin that kept crashing says so after a restart of the backend.

mod entities;

use chrono::Utc;
use entities::external_plugins as entity;
use sea_orm::sea_query::OnConflict;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, QueryOrder};
use serde::Serialize;
use utoipa::ToSchema;

use crate::services::error::ErrorService;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    /// Loaded and running (or being restarted after a crash)
    Ok,
    /// Could not be started; `last_error` says why
    Unavailable,
    /// Crashed so often that the backend gave up and switched it off; `last_error` says why
    Failing,
    /// Stopped by the owner until the next start of the backend or the next load
    Unloaded,
}

impl PluginState {
    pub fn as_str(self) -> &'static str {
        match self {
            PluginState::Ok => "ok",
            PluginState::Unavailable => "unavailable",
            PluginState::Failing => "failing",
            PluginState::Unloaded => "unloaded",
        }
    }

    fn parse(text: &str) -> Self {
        // The column's CHECK allows nothing else
        match text {
            "ok" => PluginState::Ok,
            "failing" => PluginState::Failing,
            "unloaded" => PluginState::Unloaded,
            _ => PluginState::Unavailable,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExternalPluginRecord {
    pub name: String,
    pub subname: String,
    pub version: Option<String>,
    pub kind: Option<String>,
    pub source_type: String,
    pub source_url: Option<String>,
    /// The git branch or tag it was installed from, or the commit when that is all that is known
    pub source_ref: Option<String>,
    /// The checksum of the `.tulpa` file it was installed from
    pub source_sha256: Option<String>,
    pub installed_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub state: PluginState,
    pub last_error: Option<String>,
}

/// Where an installed plugin came from
#[derive(Clone, Debug, Default)]
pub struct PluginSource {
    /// `folder`, `file` or `git`
    pub source_type: String,
    pub url: Option<String>,
    pub git_ref: Option<String>,
    pub sha256: Option<String>,
}

impl From<entity::Model> for ExternalPluginRecord {
    fn from(row: entity::Model) -> Self {
        Self {
            name: row.name,
            subname: row.subname,
            version: row.version,
            kind: row.kind,
            source_type: row.source_type,
            source_url: row.source_url,
            source_ref: row.source_ref,
            source_sha256: row.source_sha256,
            installed_at: row.installed_at,
            updated_at: row.updated_at,
            state: PluginState::parse(&row.state),
            last_error: row.last_error,
        }
    }
}

pub struct ExternalPluginStore {
    db: DatabaseConnection,
}

impl ExternalPluginStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Notes a plugin the backend has just tried to start: its version and kind when its manifest could be read, and how it
    /// went. A plugin seen for the first time is recorded as put there by hand (`folder`); one seen before keeps where it
    /// came from.
    pub async fn record(
        &self,
        name: &str,
        subname: &str,
        version: Option<&str>,
        kind: Option<&str>,
        state: PluginState,
        last_error: Option<&str>,
    ) -> Result<(), ExternalPluginStoreError> {
        let now = Utc::now();
        let row = entity::ActiveModel {
            name: Set(name.to_string()),
            subname: Set(subname.to_string()),
            version: Set(version.map(str::to_string)),
            kind: Set(kind.map(str::to_string)),
            source_type: Set("folder".to_string()),
            source_url: Set(None),
            source_ref: Set(None),
            source_sha256: Set(None),
            installed_at: Set(now),
            updated_at: Set(now),
            state: Set(state.as_str().to_string()),
            last_error: Set(last_error.map(str::to_string)),
        };
        entity::Entity::insert(row)
            .on_conflict(
                OnConflict::columns([entity::Column::Name, entity::Column::Subname])
                    .update_columns([entity::Column::Version, entity::Column::Kind, entity::Column::State, entity::Column::LastError, entity::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Notes a plugin that was installed through the installer (from a file or a git address), with where it came from. A plugin
    /// installed before keeps when it was first installed.
    pub async fn record_install(&self, name: &str, subname: &str, version: &str, kind: &str, source: &PluginSource) -> Result<(), ExternalPluginStoreError> {
        let now = Utc::now();
        let row = entity::ActiveModel {
            name: Set(name.to_string()),
            subname: Set(subname.to_string()),
            version: Set(Some(version.to_string())),
            kind: Set(Some(kind.to_string())),
            source_type: Set(source.source_type.clone()),
            source_url: Set(source.url.clone()),
            source_ref: Set(source.git_ref.clone()),
            source_sha256: Set(source.sha256.clone()),
            installed_at: Set(now),
            updated_at: Set(now),
            state: Set(PluginState::Unloaded.as_str().to_string()),
            last_error: Set(None),
        };
        entity::Entity::insert(row)
            .on_conflict(
                OnConflict::columns([entity::Column::Name, entity::Column::Subname])
                    .update_columns([
                        entity::Column::Version,
                        entity::Column::Kind,
                        entity::Column::SourceType,
                        entity::Column::SourceUrl,
                        entity::Column::SourceRef,
                        entity::Column::SourceSha256,
                        entity::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Forgets a plugin: it was uninstalled.
    pub async fn delete(&self, name: &str, subname: &str) -> Result<(), ExternalPluginStoreError> {
        entity::Entity::delete_many().filter(entity::Column::Name.eq(name)).filter(entity::Column::Subname.eq(subname)).exec(&self.db).await?;
        Ok(())
    }

    /// Changes how a plugin that is already recorded is doing. A plugin that was never recorded is left alone.
    pub async fn set_state(&self, name: &str, subname: &str, state: PluginState, last_error: Option<&str>) -> Result<(), ExternalPluginStoreError> {
        entity::Entity::update_many()
            .col_expr(entity::Column::State, Expr::value(state.as_str()))
            .col_expr(entity::Column::LastError, Expr::value(last_error.map(str::to_string)))
            .col_expr(entity::Column::UpdatedAt, Expr::value(Utc::now()))
            .filter(entity::Column::Name.eq(name))
            .filter(entity::Column::Subname.eq(subname))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<ExternalPluginRecord>, ExternalPluginStoreError> {
        let rows = entity::Entity::find().order_by_asc(entity::Column::Name).order_by_asc(entity::Column::Subname).all(&self.db).await?;
        Ok(rows.into_iter().map(ExternalPluginRecord::from).collect())
    }
}

#[derive(Debug)]
pub enum ExternalPluginStoreError {
    QueryFailed(DbErr),
}

impl From<DbErr> for ExternalPluginStoreError {
    fn from(err: DbErr) -> Self {
        ExternalPluginStoreError::QueryFailed(err)
    }
}

impl std::fmt::Display for ExternalPluginStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalPluginStoreError::QueryFailed(e) => write!(f, "external plugin store query failed: {e}"),
        }
    }
}

impl From<ExternalPluginStoreError> for ErrorService {
    fn from(err: ExternalPluginStoreError) -> Self {
        tracing::error!("{err}");
        ErrorService::internal("database query failed")
    }
}
