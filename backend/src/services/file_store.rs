mod entities;

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use chrono::Utc;
use entities::files;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection};

use crate::services::error::ErrorService;

/// Owns the mapping between a chat's files and where they actually live on disk —
/// every row is one file, under `storage_dir`. Single responsibility, kept separate
/// from `ChatStore` (message content, including the small inline-base64 images
/// attached directly to a message — this is for standalone files, not that) and from
/// `storage.*` tools (which read/write anywhere on the real filesystem the process can
/// reach; this only ever writes inside its own managed directory). Same isolated-
/// SeaORM-entity shape as the other stores — `files` is private to this module,
/// callers only ever see `FileRecord`.
pub struct FileStore {
    db: DatabaseConnection,
    storage_dir: PathBuf,
}

/// What a row actually is, decoupled from the private SeaORM `files::Model` — same
/// pattern as `ChatStore`'s `Message`.
pub struct FileRecord {
    pub id: i64,
    /// The owning user.
    pub user_id: i64,
    /// `None` for a file uploaded before a chat existed to attach it to yet — see
    /// `FileStore::attach_to_chat`.
    pub chat_id: Option<i64>,
    /// Where this file actually lives on disk, inside `storage_dir` — the name here is
    /// a generated timestamp+chat-id filename (see `store_bytes`), not anything a user
    /// picked; use `file_name` for that.
    pub full_path: String,
    /// The user/UI-facing name for this file (e.g. the original filename it was
    /// uploaded as) — independent of `full_path`, which is only ever a generated,
    /// collision-proof name on disk.
    pub file_name: String,
    /// Whether this file should be treated as immutable by callers — defaults to
    /// `true`. Advisory: `FileStore` itself doesn't enforce it (nothing here currently
    /// exposes a "write to an existing file's path" operation to enforce it against),
    /// it's state for callers (tools, routes) to check before acting on a file.
    pub read_only: bool,
}

impl From<files::Model> for FileRecord {
    fn from(model: files::Model) -> Self {
        Self {
            id: model.id,
            user_id: model.user_id,
            chat_id: model.chat_id,
            full_path: model.full_path,
            file_name: model.file_name,
            read_only: model.read_only,
        }
    }
}

impl FileStore {
    /// Holds an already-connected, already-migrated connection (see `services::bootstrap`),
    /// and ensures `storage_dir` exists on disk — every other method assumes it does.
    pub async fn new(db: DatabaseConnection, storage_dir: PathBuf) -> Self {
        tokio::fs::create_dir_all(&storage_dir).await.unwrap_or_else(|e| {
            panic!("failed to create file storage directory '{}': {e}", storage_dir.display())
        });

        Self { db, storage_dir }
    }

    /// Writes `bytes` to a new, generated path inside `storage_dir` and records it —
    /// the one-call path most callers want, instead of picking a disk name and calling
    /// `create` separately. `file_name` stays free to be whatever the UI/user actually
    /// called it — only its extension (if any) feeds the generated on-disk name.
    /// `chat_id` is `None` for a file uploaded before a chat exists to attach it to
    /// yet (the home page's case) — see `attach_to_chat`.
    pub async fn store_bytes(
        &self,
        user_id: i64,
        chat_id: Option<i64>,
        file_name: &str,
        bytes: &[u8],
        read_only: Option<bool>,
    ) -> Result<FileRecord, FileStoreErrors> {
        let full_path = self.generate_disk_path(chat_id, file_name);

        tokio::fs::write(&full_path, bytes)
            .await
            .map_err(|e| FileStoreErrors::Io(format!("couldn't write '{}': {e}", full_path.display())))?;

        self.create(user_id, chat_id, full_path.to_string_lossy().to_string(), file_name.to_string(), read_only)
            .await
    }

    /// Duplicates an existing file this store manages: a new on-disk copy of its
    /// bytes, plus a new row (same `chat_id`, `file_name`, and `read_only` as the
    /// original) pointing at it — independent from then on, deleting the original
    /// leaves the copy untouched. Errs with `NotFound` if `id` doesn't exist.
    pub async fn copy(&self, id: i64) -> Result<FileRecord, FileStoreErrors> {
        let original = self.get(id).await?;
        let new_path = self.generate_disk_path(original.chat_id, &original.file_name);

        tokio::fs::copy(&original.full_path, &new_path).await.map_err(|e| {
            FileStoreErrors::Io(format!(
                "couldn't copy '{}' to '{}': {e}",
                original.full_path,
                new_path.display()
            ))
        })?;

        self.create(
            original.user_id,
            original.chat_id,
            new_path.to_string_lossy().to_string(),
            original.file_name,
            Some(original.read_only),
        )
        .await
    }

    /// The on-disk name a brand new file gets: `<utc timestamp>[_<chat_id>]<ext>` (the
    /// extension, if any, taken from `file_name`; the chat segment omitted entirely
    /// when `chat_id` isn't known yet) — collision-proof and traceable back to its
    /// chat by eye when it has one. Shared by `store_bytes` and `copy`, the two places
    /// that mint a new path rather than reusing an existing one.
    fn generate_disk_path(&self, chat_id: Option<i64>, file_name: &str) -> PathBuf {
        let ext = Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{e}"))
            .unwrap_or_default();

        let chat_suffix = chat_id.map(|id| format!("_{id}")).unwrap_or_default();
        let disk_name = format!("{}{chat_suffix}{ext}", Utc::now().timestamp_nanos_opt().unwrap_or_default());
        self.storage_dir.join(disk_name)
    }

    /// Inserts a row for a file that already exists at `full_path` — the lower-level
    /// half of `store_bytes` (also useful on its own for a file placed on disk some
    /// other way that just needs to be registered). Doesn't touch the filesystem.
    pub async fn create(
        &self,
        user_id: i64,
        chat_id: Option<i64>,
        full_path: String,
        file_name: String,
        read_only: Option<bool>,
    ) -> Result<FileRecord, FileStoreErrors> {
        let model = files::ActiveModel {
            user_id: Set(user_id),
            chat_id: Set(chat_id),
            full_path: Set(full_path),
            file_name: Set(file_name),
            read_only: Set(read_only.unwrap_or(true)),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        Ok(model.into())
    }

    /// Errs with `NotFound` if no row has this id.
    pub async fn get(&self, id: i64) -> Result<FileRecord, FileStoreErrors> {
        let row = files::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(FileStoreErrors::NotFound)?;

        Ok(row.into())
    }

    /// Every file recorded for a chat. Empty, not an error, for a chat with none.
    pub async fn list_by_chat(&self, chat_id: i64) -> Result<Vec<FileRecord>, FileStoreErrors> {
        let rows = files::Entity::find()
            .filter(files::Column::ChatId.eq(chat_id))
            .all(&self.db)
            .await?;

        Ok(rows.into_iter().map(FileRecord::from).collect())
    }

    /// Updates the mutable, UI-facing fields of a file's row — its display name and/or
    /// its `read_only` flag. `chat_id` and `full_path` are deliberately not editable
    /// here: they're what identifies which physical file a row is, not display state.
    /// `None` for either field leaves it unchanged. Errs with `NotFound` if no row has
    /// this id.
    pub async fn update(
        &self,
        id: i64,
        file_name: Option<String>,
        read_only: Option<bool>,
    ) -> Result<FileRecord, FileStoreErrors> {
        let existing = files::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(FileStoreErrors::NotFound)?;

        let mut model: files::ActiveModel = existing.into();
        if let Some(file_name) = file_name {
            model.file_name = Set(file_name);
        }
        if let Some(read_only) = read_only {
            model.read_only = Set(read_only);
        }

        let updated = model.update(&self.db).await?;
        Ok(updated.into())
    }

    /// Sets a file's `chat_id` — for claiming one uploaded before a chat existed yet
    /// (`chat_id: None`, the home page's case) the moment it's actually attached to a
    /// message in a real chat. Unconditional (doesn't check the file's current
    /// `chat_id` first): the one caller today (`Agent::chat`) only ever calls this for
    /// a `file_id` the message itself just referenced, so there's no meaningful
    /// "already claimed by someone else" case to guard against yet. Errs with
    /// `NotFound` if no row has this id.
    pub async fn attach_to_chat(&self, id: i64, chat_id: i64) -> Result<FileRecord, FileStoreErrors> {
        let existing = files::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(FileStoreErrors::NotFound)?;

        let mut model: files::ActiveModel = existing.into();
        model.chat_id = Set(Some(chat_id));

        let updated = model.update(&self.db).await?;
        Ok(updated.into())
    }

    /// Deletes a file's row and best-effort removes the underlying file from disk.
    /// A missing file on disk (already gone some other way) doesn't fail the call —
    /// the row being gone afterward is what callers actually care about; a real I/O
    /// failure (permissions, disk error) is only logged, for the same reason: the
    /// alternative is a row nothing can ever delete because its file is stuck. Errs
    /// with `NotFound` only if there was no row to begin with.
    pub async fn delete(&self, id: i64) -> Result<(), FileStoreErrors> {
        let existing = files::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .ok_or(FileStoreErrors::NotFound)?;

        if let Err(e) = tokio::fs::remove_file(&existing.full_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("couldn't remove file '{}' for file id {id}: {e}", existing.full_path);
            }
        }

        files::Entity::delete_by_id(id).exec(&self.db).await?;

        Ok(())
    }
}

/// Mirrors `PermissionStoreErrors`'s shape, plus `Io` for the filesystem half of
/// `store_bytes`/`delete` — kept separate from `QueryFailed` since it's never a
/// database problem.
#[derive(Debug)]
pub enum FileStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    Io(String),
}

impl From<DbErr> for FileStoreErrors {
    fn from(err: DbErr) -> Self {
        FileStoreErrors::QueryFailed(err)
    }
}

impl From<FileStoreErrors> for ErrorService {
    fn from(err: FileStoreErrors) -> Self {
        match err {
            FileStoreErrors::QueryFailed(e) => {
                tracing::error!("file store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            FileStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "no such file"),
            FileStoreErrors::Io(e) => {
                tracing::error!("file store I/O failed: {e}");
                ErrorService::internal("filesystem operation failed")
            }
        }
    }
}
