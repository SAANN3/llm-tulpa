mod entities;

use axum::http::StatusCode;
use entities::folders;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, DbBackend, FromQueryResult, Statement};

use crate::services::error::ErrorService;

/// Owns a user's chat folders — a flat, optional grouping (`chats.folder_id`) with no
/// independent history of its own, so unlike `ChatStore` this has no soft delete: removing
/// a folder just removes the row, and `chats.folder_id`'s `ON DELETE SET NULL` ungroups
/// its chats rather than deleting them. Same isolated-SeaORM-entity shape as the other
/// stores: `folders` is private to this module, callers only ever see `Folder`.
pub struct FolderStore {
    db: DatabaseConnection,
}

pub struct Folder {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl From<folders::Model> for Folder {
    fn from(model: folders::Model) -> Self {
        Self {
            id: model.id,
            user_id: model.user_id,
            name: model.name,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

impl FolderStore {
    /// Holds an already-connected, already-migrated connection (see `services::bootstrap`).
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// A folder by id — does NOT check ownership; callers acting on behalf of a user use
    /// `owned_folder` instead.
    pub async fn folder(&self, folder_id: i64) -> Result<Folder, FolderStoreErrors> {
        folders::Entity::find_by_id(folder_id)
            .one(&self.db)
            .await?
            .map(Folder::from)
            .ok_or(FolderStoreErrors::NotFound)
    }

    /// Like `folder`, but 404s unless the folder belongs to `user_id` — the ownership gate
    /// handlers (and `ChatStore::set_folder`) use before acting on a folder.
    pub async fn owned_folder(&self, user_id: i64, folder_id: i64) -> Result<Folder, FolderStoreErrors> {
        let folder = self.folder(folder_id).await?;
        if folder.user_id != user_id {
            return Err(FolderStoreErrors::NotFound);
        }
        Ok(folder)
    }

    /// A user's folders, ordered by their most recently active chat (falling back to the
    /// folder's own `updated_at` when it has none), optionally filtered to names
    /// containing `name_filter` (case-insensitive), plus the total match count for
    /// pagination. A user is expected to have only a handful of folders in practice, but
    /// nothing stops it growing, so this is paginated like every other user-owned list
    /// rather than assuming it stays small. Raw SQL (not the query builder) because the
    /// sort key is a join+aggregate over another store's table, which `Select` can't
    /// express while still returning plain `folders` rows.
    pub async fn folders(
        &self,
        user_id: i64,
        name_filter: Option<&str>,
        limit: u64,
        skip: u64,
    ) -> Result<(Vec<Folder>, u64), FolderStoreErrors> {
        // Escape LIKE wildcards so a searched `%` or `_` matches itself, not anything —
        // same convention as `ChatStore::search_messages`.
        let pattern = name_filter.map(|q| format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));

        let total_row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT COUNT(*) AS count FROM folders WHERE user_id = $1 AND ($2::text IS NULL OR name ILIKE $2)",
                [user_id.into(), pattern.clone().into()],
            ))
            .await?;
        let total: i64 = match total_row {
            Some(row) => row.try_get("", "count")?,
            None => 0,
        };

        let rows = self
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT f.* FROM folders f
                 LEFT JOIN chats c ON c.folder_id = f.id AND c.is_deleted = false
                 WHERE f.user_id = $1 AND ($2::text IS NULL OR f.name ILIKE $2)
                 GROUP BY f.id
                 ORDER BY COALESCE(MAX(c.updated_at), f.updated_at) DESC
                 LIMIT $3 OFFSET $4",
                [user_id.into(), pattern.into(), (limit as i64).into(), (skip as i64).into()],
            ))
            .await?;

        let folders = rows
            .iter()
            .map(|row| folders::Model::from_query_result(row, "").map(Folder::from))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((folders, total.max(0) as u64))
    }

    pub async fn create_folder(&self, user_id: i64, name: String) -> Result<Folder, FolderStoreErrors> {
        let row = folders::ActiveModel { user_id: Set(user_id), name: Set(name), ..Default::default() }
            .insert(&self.db)
            .await?;

        Ok(row.into())
    }

    /// Renames the given folder. Ownership is the caller's responsibility.
    pub async fn rename_folder(&self, folder_id: i64, name: String) -> Result<(), FolderStoreErrors> {
        self.folder(folder_id).await?;
        folders::ActiveModel {
            id: Set(folder_id),
            name: Set(name),
            updated_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .update(&self.db)
        .await?;
        Ok(())
    }

    /// Deletes the folder. Its chats aren't touched here — `chats.folder_id`'s
    /// `ON DELETE SET NULL` ungroups them at the database level. Ownership is the
    /// caller's responsibility.
    pub async fn delete_folder(&self, folder_id: i64) -> Result<(), FolderStoreErrors> {
        self.folder(folder_id).await?;
        folders::Entity::delete_by_id(folder_id).exec(&self.db).await?;
        Ok(())
    }
}

/// Wraps every SeaORM failure uniformly, same pattern as `ChatStoreErrors`.
#[derive(Debug)]
pub enum FolderStoreErrors {
    QueryFailed(DbErr),
    NotFound,
}

impl From<DbErr> for FolderStoreErrors {
    fn from(err: DbErr) -> Self {
        FolderStoreErrors::QueryFailed(err)
    }
}

impl From<FolderStoreErrors> for ErrorService {
    fn from(err: FolderStoreErrors) -> Self {
        match err {
            FolderStoreErrors::QueryFailed(e) => {
                tracing::error!("folder store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            FolderStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "folder not found"),
        }
    }
}
