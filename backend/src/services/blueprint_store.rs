mod entities;
mod runs;
mod agent_chats;
mod schedules;
mod variables;
pub use schedules::ScheduledTick;

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveValue::Set, DatabaseConnection, QueryOrder, QuerySelect, Select, TransactionTrait, prelude::*,
    sea_query::{Alias, Func},
};
use tokio::io::AsyncWriteExt;

use crate::facade::blueprint::{definition::Schedule, runs::RunStatus, validation::MAX_DOCUMENT_BYTES};
use crate::services::error::ErrorService;
use entities::{blueprint_runs, blueprints as entity};

pub struct BlueprintStore {
    db: DatabaseConnection,
    root: PathBuf,
}

pub struct BlueprintRecord {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub revision: i64,
    pub schedule: Schedule,
    pub next_due_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<entity::Model> for BlueprintRecord {
    type Error = BlueprintStoreError;
    fn try_from(row: entity::Model) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            revision: row.revision,
            schedule: serde_json::from_value(row.schedule)?,
            next_due_at: row.next_due_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl BlueprintStore {
    pub fn new(db: DatabaseConnection, root: PathBuf) -> Self {
        Self { db, root }
    }

    fn owned(user_id: i64, id: i64) -> Select<entity::Entity> {
        entity::Entity::find_by_id(id).filter(entity::Column::UserId.eq(user_id))
    }

    pub async fn list(
        &self,
        user_id: i64,
        offset: u64,
        limit: u64,
        query: &str,
        updated: bool,
    ) -> Result<(Vec<BlueprintRecord>, u64), BlueprintStoreError> {
        let mut rows = entity::Entity::find().filter(entity::Column::UserId.eq(user_id));
        use sea_orm::sea_query::extension::postgres::PgExpr as _;

        let name = Expr::expr(Func::cust(Alias::new("regexp_replace")).args([
            Expr::col(entity::Column::Name),
            Expr::val("[[:space:]]+"),
            Expr::val(""),
            Expr::val("g"),
        ]));
        for word in query.split_whitespace() {
            let escaped = word
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            rows = rows.filter(name.clone().ilike(format!("%{escaped}%")));
        }
        let total = rows.clone().count(&self.db).await?;
        let records = rows
            .order_by_desc(if updated {
                entity::Column::UpdatedAt
            } else {
                entity::Column::CreatedAt
            })
            .order_by_desc(entity::Column::Id)
            .offset(offset)
            .limit(limit)
            .all(&self.db)
            .await?
            .into_iter()
            .map(BlueprintRecord::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((records, total))
    }

    pub async fn get(
        &self,
        user_id: i64,
        id: i64,
    ) -> Result<(BlueprintRecord, Vec<u8>), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        // Keep delete from removing the directory while its document is being read.
        let row = Self::owned(user_id, id)
            .lock_shared()
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        let prefix = format!("{user_id}/{id}/workflow-");
        let file_revision = row
            .workflow_path
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|r| *r > 0 && *r <= row.revision)
            .ok_or_else(|| BlueprintStoreError::Corrupt("unexpected workflow path".into()))?;
        let expected = Self::relative_path(user_id, id, file_revision);
        if row.workflow_path != expected {
            return Err(BlueprintStoreError::Corrupt(
                "unexpected workflow path".into(),
            ));
        }
        let root = tokio::fs::canonicalize(&self.root).await?;
        let path = root.join(&expected);
        if tokio::fs::canonicalize(&path).await? != path {
            return Err(BlueprintStoreError::Corrupt(
                "workflow path follows a symlink".into(),
            ));
        }
        if tokio::fs::metadata(&path).await?.len() > MAX_DOCUMENT_BYTES as u64 {
            return Err(BlueprintStoreError::Corrupt(
                "workflow document is too large".into(),
            ));
        }
        let bytes = tokio::fs::read(path).await?;
        txn.commit().await?;
        Ok((row.try_into()?, bytes))
    }

    pub async fn create(
        &self,
        user_id: i64,
        name: &str,
        schedule: &Schedule,
        document: &[u8],
    ) -> Result<BlueprintRecord, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        let row = entity::ActiveModel {
            user_id: Set(user_id),
            name: Set(name.to_string()),
            revision: Set(1),
            workflow_path: Set(String::new()),
            schedule: Set(serde_json::to_value(schedule)?),
            next_due_at: Set(schedule.next_after(Utc::now()).map_err(BlueprintStoreError::Corrupt)?),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        let path = self.write_document(user_id, row.id, 1, document).await?;
        let mut row: entity::ActiveModel = row.into();
        row.workflow_path = Set(path);
        let row = row.update(&txn).await?;
        // A failed/ambiguous commit may leave an unreferenced revision. Never remove a file that
        // could have been committed; the previous referenced revision is never overwritten.
        txn.commit().await?;
        row.try_into()
    }

    /// Metadata-only updates advance the same revision without rewriting the document.
    pub async fn update(
        &self,
        user_id: i64,
        id: i64,
        expected_revision: i64,
        name: &str,
        schedule: &Schedule,
        document: Option<&[u8]>,
    ) -> Result<BlueprintRecord, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        let row = Self::owned(user_id, id)
            .lock_exclusive()
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if row.revision != expected_revision {
            return Err(BlueprintStoreError::Conflict);
        }
        let revision = row
            .revision
            .checked_add(1)
            .ok_or_else(|| BlueprintStoreError::Corrupt("revision overflow".into()))?;
        let path = if let Some(bytes) = document {
            self.write_document(user_id, id, revision, bytes).await?
        } else {
            row.workflow_path.clone()
        };
        let next_due = if serde_json::from_value::<Schedule>(row.schedule.clone())? == *schedule {
            row.next_due_at
        } else {
            schedule.next_after(Utc::now()).map_err(BlueprintStoreError::Corrupt)?
        };
        let mut row: entity::ActiveModel = row.into();
        row.next_due_at = Set(next_due);
        row.name = Set(name.to_string());
        row.schedule = Set(serde_json::to_value(schedule)?);
        row.revision = Set(revision);
        row.workflow_path = Set(path);
        row.updated_at = Set(Utc::now());
        let row = row.update(&txn).await?;
        txn.commit().await?;
        row.try_into()
    }

    pub async fn delete(
        &self,
        user_id: i64,
        id: i64,
        expected_revision: i64,
    ) -> Result<(), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        let row = Self::owned(user_id, id)
            .lock_exclusive()
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if row.revision != expected_revision {
            return Err(BlueprintStoreError::Conflict);
        }
        if blueprint_runs::Entity::find()
            .filter(blueprint_runs::Column::BlueprintId.eq(id))
            .filter(blueprint_runs::Column::Status.is_in([
                RunStatus::Queued,
                RunStatus::Running,
                RunStatus::WaitingForPermission,
                RunStatus::Stopping,
            ])).one(&txn).await?.is_some() {
            return Err(BlueprintStoreError::Busy);
        }
        entity::Entity::delete_by_id(id).exec(&txn).await?;
        txn.commit().await?;
        // The row is already gone. A cleanup failure must not turn a completed delete into a retry.
        if let Err(error) = self.remove_directory(user_id, id).await {
            tracing::warn!(
                user_id,
                blueprint_id = id,
                ?error,
                "could not remove blueprint files"
            );
        }
        Ok(())
    }

    async fn remove_directory(&self, user_id: i64, id: i64) -> Result<(), BlueprintStoreError> {
        let root = tokio::fs::canonicalize(&self.root).await?;
        let dir = root.join(user_id.to_string()).join(id.to_string());
        if tokio::fs::canonicalize(&dir).await? != dir {
            return Err(BlueprintStoreError::Corrupt(
                "blueprint directory follows a symlink".into(),
            ));
        }
        tokio::fs::remove_dir_all(dir).await?;
        Ok(())
    }

    fn relative_path(user_id: i64, id: i64, revision: i64) -> String {
        format!("{user_id}/{id}/workflow-{revision}.json")
    }

    async fn write_document(
        &self,
        user_id: i64,
        id: i64,
        revision: i64,
        bytes: &[u8],
    ) -> Result<String, BlueprintStoreError> {
        tokio::fs::create_dir_all(&self.root).await?;
        let root = tokio::fs::canonicalize(&self.root).await?;
        let user = root.join(user_id.to_string());
        Self::directory(&user).await?;
        let dir = user.join(id.to_string());
        Self::directory(&dir).await?;
        let relative = Self::relative_path(user_id, id, revision);
        let temp = dir.join(format!(".write-{:016x}", rand::random::<u64>()));
        let result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)
                .await?;
            file.write_all(bytes).await?;
            file.sync_all().await?;
            drop(file);
            tokio::fs::rename(&temp, root.join(&relative)).await
        }
        .await;
        if result.is_err() {
            // A partial temporary file has no row referring to it.
            let _ = tokio::fs::remove_file(&temp).await;
        }
        result?;
        Ok(relative)
    }

    async fn directory(path: &Path) -> Result<(), BlueprintStoreError> {
        if let Err(error) = tokio::fs::create_dir(path).await {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.into());
            }
        }
        if tokio::fs::canonicalize(path).await? != path
            || !tokio::fs::metadata(path).await?.is_dir()
        {
            return Err(BlueprintStoreError::Corrupt(
                "blueprint directory must not be a symlink".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum BlueprintStoreError {
    NotFound,
    Conflict,
    QueueChanged,
    NotQueued,
    ConfirmationChanged,
    Busy,
    VariablesBusy,
    VariableState(String),
    Database(DbErr),
    Io(std::io::Error),
    Json(serde_json::Error),
    Corrupt(String),
}
impl From<DbErr> for BlueprintStoreError {
    fn from(e: DbErr) -> Self {
        Self::Database(e)
    }
}
impl From<std::io::Error> for BlueprintStoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for BlueprintStoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
impl From<BlueprintStoreError> for ErrorService {
    fn from(error: BlueprintStoreError) -> Self {
        match error {
            BlueprintStoreError::NotFound => Self::new(StatusCode::NOT_FOUND, "no such blueprint"),
            BlueprintStoreError::Conflict => Self::new(
                StatusCode::CONFLICT,
                "blueprint changed; reload before saving or deleting",
            ),
            BlueprintStoreError::ConfirmationChanged => Self::new(StatusCode::CONFLICT, "tool confirmation changed; reload the run before answering"),
            BlueprintStoreError::QueueChanged => Self::new(StatusCode::CONFLICT, "queue changed; reload before reordering"),
            BlueprintStoreError::NotQueued => Self::new(StatusCode::CONFLICT, "that run is no longer waiting"),
            BlueprintStoreError::VariablesBusy => Self::new(StatusCode::CONFLICT, "stop the active run before resetting this blueprint's variables"),
            BlueprintStoreError::VariableState(message) => Self::new(StatusCode::UNPROCESSABLE_ENTITY, message),
            BlueprintStoreError::Busy => Self::new(StatusCode::CONFLICT, "stop the current run and clear the queue before deleting this blueprint"),
            BlueprintStoreError::Database(e) => {
                tracing::error!(?e, "blueprint database operation failed");
                Self::internal("blueprint database operation failed")
            }
            BlueprintStoreError::Io(e) => {
                tracing::error!(?e, "blueprint file operation failed");
                Self::internal("blueprint file operation failed")
            }
            BlueprintStoreError::Json(e) => {
                tracing::error!(?e, "invalid stored blueprint");
                Self::internal("invalid stored blueprint")
            }
            BlueprintStoreError::Corrupt(e) => {
                tracing::error!(%e, "invalid stored blueprint");
                Self::internal("invalid stored blueprint")
            }
        }
    }
}
