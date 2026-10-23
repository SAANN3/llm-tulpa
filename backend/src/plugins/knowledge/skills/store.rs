//! Where skills are kept: one row per skill in the plugin's own table (name, description, who made it) and the skill's files
//! in a folder, `<root>/<user id>/<skill name>/`. The row is what the prompt and the list read; the folder is what the
//! reading tools open. A skill's name is the folder's name, so it is checked (`format::valid_name`) before any path is built
//! from it, and every file path is cleaned (`paths::clean`) and then confirmed to still be inside the skill's folder after
//! symlinks are resolved.
//!
//! Who made a skill decides who may change it: a skill the user uploaded (`Origin::User`) is replaced or deleted only by
//! the user; one the model made (`Origin::Model`) may be rewritten or deleted by either.

use std::path::{Path, PathBuf};

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, QueryOrder};
use tokio::sync::Mutex;

use super::format;
use super::pack::{MAX_FILES, MAX_FILE_BYTES, MAX_TOTAL_BYTES, SKILL_FILE};
use super::paths;

mod entities;
use entities::items as entity;

/// The table's SQL, for `PluginBuilder::migrations`; `prefix` is the plugin's table prefix, which makes the table the
/// `plugin_knowledge_skills_items` `entities/items.rs` names.
pub fn migrations(prefix: &str) -> Vec<String> {
    vec![format!(
        "CREATE TABLE {prefix}_items (
            id BIGSERIAL PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            origin TEXT NOT NULL CHECK (origin IN ('user', 'model')),
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (user_id, name)
        )"
    )]
}

/// Who made a skill, or who is acting on one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    User,
    Model,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::User => "user",
            Origin::Model => "model",
        }
    }

    fn parse(text: &str) -> Self {
        // The column's CHECK allows nothing else
        if text == "user" { Origin::User } else { Origin::Model }
    }
}

#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub origin: Origin,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl From<entity::Model> for Skill {
    fn from(row: entity::Model) -> Self {
        Self {
            name: row.name,
            description: row.description,
            origin: Origin::parse(&row.origin),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug)]
pub enum SkillStoreError {
    NotFound(String),
    /// The request is fine but not allowed (a skill the user made, asked to be changed by the model)
    Refused(String),
    /// The name, a path or the files break a rule
    Invalid(String),
    Io(String),
    Db(DbErr),
}

impl std::fmt::Display for SkillStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillStoreError::NotFound(what) => write!(f, "{what} was not found"),
            SkillStoreError::Refused(why) | SkillStoreError::Invalid(why) | SkillStoreError::Io(why) => write!(f, "{why}"),
            SkillStoreError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl From<DbErr> for SkillStoreError {
    fn from(e: DbErr) -> Self {
        SkillStoreError::Db(e)
    }
}

fn io(what: &str, e: std::io::Error) -> SkillStoreError {
    SkillStoreError::Io(format!("{what}: {e}"))
}

pub struct SkillStore {
    db: DatabaseConnection,
    root: PathBuf,
    /// Held across a write or a delete so the check of who made a skill and the change that follows it cannot be split
    /// by another request for the same skill; there is one backend process, so a lock here is enough.
    write_lock: Mutex<()>,
}

impl SkillStore {
    pub fn new(db: DatabaseConnection, root: PathBuf) -> Self {
        Self { db, root, write_lock: Mutex::new(()) }
    }

    /// The folder of one user's skills
    pub fn user_dir(&self, user_id: i64) -> PathBuf {
        self.root.join(user_id.to_string())
    }

    /// The folder of a skill; the name is checked first because it becomes a path.
    fn skill_dir(&self, user_id: i64, name: &str) -> Result<PathBuf, SkillStoreError> {
        if !format::valid_name(name) {
            return Err(SkillStoreError::Invalid(format!(
                "'{name}' is not a skill name; use lowercase letters and digits in words joined by hyphens, at most {} characters",
                format::MAX_NAME_CHARS
            )));
        }
        Ok(self.user_dir(user_id).join(name))
    }

    async fn row(&self, user_id: i64, name: &str) -> Result<Option<entity::Model>, SkillStoreError> {
        Ok(entity::Entity::find()
            .filter(entity::Column::UserId.eq(user_id))
            .filter(entity::Column::Name.eq(name))
            .one(&self.db)
            .await?)
    }

    pub async fn list(&self, user_id: i64) -> Result<Vec<Skill>, SkillStoreError> {
        let rows = entity::Entity::find()
            .filter(entity::Column::UserId.eq(user_id))
            .order_by_asc(entity::Column::Name)
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(Skill::from).collect())
    }

    pub async fn get(&self, user_id: i64, name: &str) -> Result<Skill, SkillStoreError> {
        self.skill_dir(user_id, name)?;
        self.row(user_id, name)
            .await?
            .map(Skill::from)
            .ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))
    }

    /// The paths of every file in a skill, from its own folder, sorted.
    pub async fn files(&self, user_id: i64, name: &str) -> Result<Vec<String>, SkillStoreError> {
        self.get(user_id, name).await?;
        let dir = self.skill_dir(user_id, name)?;
        let mut found = Vec::new();
        let mut pending = vec![(dir.clone(), String::new())];
        while let Some((folder, prefix)) = pending.pop() {
            let mut entries = tokio::fs::read_dir(&folder).await.map_err(|e| io("could not list the skill's folder", e))?;
            while let Some(entry) = entries.next_entry().await.map_err(|e| io("could not list the skill's folder", e))? {
                let file_type = entry.file_type().await.map_err(|e| io("could not list the skill's folder", e))?;
                let path = format!("{prefix}{}", entry.file_name().to_string_lossy());
                // A link is skipped: it could point out of the folder, and a skill written by this store has none
                if file_type.is_dir() {
                    pending.push((entry.path(), format!("{path}/")));
                } else if file_type.is_file() {
                    found.push(path);
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// One file of a skill. The path is cleaned, then the file is resolved on disk and refused if it ends up outside the
    /// skill's folder (a link put there by hand would do that).
    pub async fn read_file(&self, user_id: i64, name: &str, path: &str) -> Result<Vec<u8>, SkillStoreError> {
        self.get(user_id, name).await?;
        let path = paths::clean(path).map_err(SkillStoreError::Invalid)?;
        let dir = self.skill_dir(user_id, name)?;
        let wanted = dir.join(&path);
        let real = tokio::fs::canonicalize(&wanted)
            .await
            .map_err(|_| SkillStoreError::NotFound(format!("file '{path}' in skill '{name}'")))?;
        let real_dir = tokio::fs::canonicalize(&dir).await.map_err(|e| io("could not open the skill's folder", e))?;
        if !real.starts_with(&real_dir) {
            return Err(SkillStoreError::Refused(format!("'{path}' leads out of the skill's folder")));
        }
        let meta = tokio::fs::metadata(&real).await.map_err(|e| io("could not read the file", e))?;
        if !meta.is_file() {
            return Err(SkillStoreError::NotFound(format!("file '{path}' in skill '{name}'")));
        }
        tokio::fs::read(&real).await.map_err(|e| io("could not read the file", e))
    }

    /// Whether `by` may change `existing`.
    fn allowed(existing: &entity::Model, by: Origin, name: &str) -> Result<(), SkillStoreError> {
        if by == Origin::Model && Origin::parse(&existing.origin) == Origin::User {
            return Err(SkillStoreError::Refused(format!(
                "skill '{name}' was added by the user, so only the user can change it; use another name"
            )));
        }
        Ok(())
    }

    /// Stores a skill, replacing one of the same name `by` is allowed to change. `files` are named from the skill's own
    /// folder and must hold the `SKILL.md`; the skill's folder is built beside the old one and swapped in, so a failure
    /// part-way leaves the old skill as it was.
    pub async fn save(
        &self,
        user_id: i64,
        name: &str,
        description: &str,
        files: Vec<(String, Vec<u8>)>,
        by: Origin,
    ) -> Result<Skill, SkillStoreError> {
        let dir = self.skill_dir(user_id, name)?;
        let files = checked_files(files)?;
        // One line wherever it is shown: the prompt lists skills a line each
        let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
        if description.is_empty() {
            return Err(SkillStoreError::Invalid("a skill needs a description that says when to use it".to_string()));
        }

        let _guard = self.write_lock.lock().await;
        let existing = self.row(user_id, name).await?;
        if let Some(existing) = &existing {
            Self::allowed(existing, by, name)?;
        }

        let user_dir = self.user_dir(user_id);
        let salt = format!("{:016x}", rand::random::<u64>());
        let staging = user_dir.join(format!(".new-{name}-{salt}"));
        let backup = user_dir.join(format!(".old-{name}-{salt}"));

        let written = write_all(&staging, &files).await;
        if let Err(e) = written {
            let _ = tokio::fs::remove_dir_all(&staging).await; // best effort: what is left is only the half-written copy
            return Err(e);
        }

        let had_old = tokio::fs::try_exists(&dir).await.unwrap_or(false);
        if had_old {
            if let Err(e) = tokio::fs::rename(&dir, &backup).await {
                let _ = tokio::fs::remove_dir_all(&staging).await;
                return Err(io("could not replace the old skill", e));
            }
        }
        if let Err(e) = tokio::fs::rename(&staging, &dir).await {
            if had_old {
                let _ = tokio::fs::rename(&backup, &dir).await; // put the old one back
            }
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(io("could not put the skill in place", e));
        }

        let now = Utc::now();
        let row = entity::ActiveModel {
            user_id: Set(user_id),
            name: Set(name.to_string()),
            description: Set(description),
            origin: Set(by.as_str().to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let saved = entity::Entity::insert(row)
            .on_conflict(
                OnConflict::columns([entity::Column::UserId, entity::Column::Name])
                    .update_columns([entity::Column::Description, entity::Column::Origin, entity::Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await;
        if let Err(e) = saved {
            // The row did not change, so the folder goes back to match it
            let _ = tokio::fs::remove_dir_all(&dir).await;
            if had_old {
                let _ = tokio::fs::rename(&backup, &dir).await;
            }
            return Err(e.into());
        }
        if had_old {
            let _ = tokio::fs::remove_dir_all(&backup).await; // best effort: a leftover `.old-` folder is only wasted space
        }
        self.get(user_id, name).await
    }

    /// Adds or replaces one file of a skill. `SKILL.md` is not one of them: its header is what the row is made from, so
    /// changing it goes through `save`.
    pub async fn add_file(&self, user_id: i64, name: &str, path: &str, bytes: Vec<u8>, by: Origin) -> Result<(), SkillStoreError> {
        let dir = self.skill_dir(user_id, name)?;
        let path = paths::clean(path).map_err(SkillStoreError::Invalid)?;
        if path == SKILL_FILE {
            return Err(SkillStoreError::Invalid(format!("{SKILL_FILE} is changed by writing the whole skill again")));
        }
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SkillStoreError::Invalid(format!("'{path}' is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024)));
        }

        let _guard = self.write_lock.lock().await;
        let existing = self
            .row(user_id, name)
            .await?
            .ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))?;
        Self::allowed(&existing, by, name)?;

        let have = self.files(user_id, name).await?;
        if !have.contains(&path) && have.len() >= MAX_FILES {
            return Err(SkillStoreError::Invalid(format!("a skill holds at most {MAX_FILES} files")));
        }

        let target = dir.join(&path);
        // A path through a link already in the folder would leave it; the parent is resolved before anything is written
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| io("could not make the folder", e))?;
            let real_parent = tokio::fs::canonicalize(parent).await.map_err(|e| io("could not open the folder", e))?;
            let real_dir = tokio::fs::canonicalize(&dir).await.map_err(|e| io("could not open the skill's folder", e))?;
            if !real_parent.starts_with(&real_dir) {
                return Err(SkillStoreError::Refused(format!("'{path}' leads out of the skill's folder")));
            }
        }
        tokio::fs::write(&target, bytes).await.map_err(|e| io("could not write the file", e))?;

        entity::Entity::update_many()
            .col_expr(entity::Column::UpdatedAt, Expr::value(Utc::now()))
            .filter(entity::Column::Id.eq(existing.id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Deletes a skill `by` is allowed to change: the row first (so it drops out of the prompt at once), then the folder.
    pub async fn delete(&self, user_id: i64, name: &str, by: Origin) -> Result<(), SkillStoreError> {
        let dir = self.skill_dir(user_id, name)?;
        let _guard = self.write_lock.lock().await;
        let existing = self
            .row(user_id, name)
            .await?
            .ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))?;
        Self::allowed(&existing, by, name)?;
        entity::Entity::delete_by_id(existing.id).exec(&self.db).await?;
        // The row is gone, so a folder that cannot be removed now is only wasted space, not a skill that still shows
        if let Err(e) = tokio::fs::remove_dir_all(&dir).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("skill folder {} could not be removed: {e}", dir.display());
            }
        }
        Ok(())
    }
}

/// The files cleaned, with the limits applied and a `SKILL.md` among them.
fn checked_files(files: Vec<(String, Vec<u8>)>) -> Result<Vec<(String, Vec<u8>)>, SkillStoreError> {
    if files.len() > MAX_FILES {
        return Err(SkillStoreError::Invalid(format!("a skill holds at most {MAX_FILES} files")));
    }
    let mut total = 0usize;
    let mut out: Vec<(String, Vec<u8>)> = Vec::with_capacity(files.len());
    for (path, bytes) in files {
        let path = paths::clean(&path).map_err(SkillStoreError::Invalid)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(SkillStoreError::Invalid(format!("'{path}' is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024)));
        }
        total += bytes.len();
        if total > MAX_TOTAL_BYTES {
            return Err(SkillStoreError::Invalid(format!("a skill is at most {} MB in all", MAX_TOTAL_BYTES / 1024 / 1024)));
        }
        if out.iter().any(|(seen, _)| *seen == path) {
            return Err(SkillStoreError::Invalid(format!("'{path}' appears twice")));
        }
        out.push((path, bytes));
    }
    if !out.iter().any(|(path, _)| path == SKILL_FILE) {
        return Err(SkillStoreError::Invalid(format!("there is no {SKILL_FILE}")));
    }
    Ok(out)
}

async fn write_all(folder: &Path, files: &[(String, Vec<u8>)]) -> Result<(), SkillStoreError> {
    for (path, bytes) in files {
        let target = folder.join(path);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| io("could not make the skill's folder", e))?;
        }
        tokio::fs::write(&target, bytes).await.map_err(|e| io("could not write the skill's files", e))?;
    }
    Ok(())
}
