//! Where skills are kept: one row per skill in the plugin's own table (name, description, who made it) and the skill's files
//! in a folder, `<data dir>/<user id>/<skill name>/`. The row is what the prompt and the list read; the folder is what the
//! reading tools open. A skill's name is the folder's name, so it is checked (`format::valid_name`) before any path is built
//! from it, and every file path is cleaned (`paths::clean`) and then confirmed to still be inside the skill's folder after
//! symlinks are resolved.
//!
//! Who made a skill decides who may change it: a skill the user uploaded (`Origin::User`) is replaced or deleted only by
//! the user; one the model made (`Origin::Model`) may be rewritten or deleted by either.
//!
//! A skill the user turned off (`enabled`) stays in the list and on disk, but is not the model's: the prompt and the tools
//! leave it out, and the model may not change it.

use std::path::{Path, PathBuf};

use tokio::sync::Mutex;
use tulpa_plugin::{Host, Value, json};

use crate::format;
use crate::pack::{MAX_FILES, MAX_FILE_BYTES, MAX_TOTAL_BYTES, SKILL_FILE};
use crate::paths;

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
    /// Whether the model may see it; the user's own list shows every skill either way
    pub enabled: bool,
}

/// A skill with the id of its row
struct Row {
    id: i64,
    skill: Skill,
}

impl Row {
    fn of(row: &Value) -> Result<Self, SkillStoreError> {
        let text = |key: &str| row[key].as_str().map(str::to_string).ok_or_else(|| SkillStoreError::Db(format!("a skill row has no {key}")));
        Ok(Self {
            id: row["id"].as_i64().ok_or_else(|| SkillStoreError::Db("a skill row has no id".to_string()))?,
            skill: Skill {
                name: text("name")?,
                description: text("description")?,
                origin: Origin::parse(&text("origin")?),
                enabled: row["enabled"].as_bool().unwrap_or(true),
            },
        })
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
    Db(String),
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

impl From<tulpa_plugin::Error> for SkillStoreError {
    fn from(e: tulpa_plugin::Error) -> Self {
        SkillStoreError::Db(e.0)
    }
}

/// A skill that is not there is a not-found, a refused request a bad request; only a failure of the disk or the database is
/// the plugin's own.
impl From<SkillStoreError> for tulpa_plugin::Failure {
    fn from(e: SkillStoreError) -> Self {
        match e {
            SkillStoreError::NotFound(_) => tulpa_plugin::Failure::not_found(e.to_string()),
            SkillStoreError::Refused(_) | SkillStoreError::Invalid(_) => tulpa_plugin::Failure::bad_args(e.to_string()),
            SkillStoreError::Io(_) | SkillStoreError::Db(_) => e.to_string().into(),
        }
    }
}

fn io(what: &str, e: std::io::Error) -> SkillStoreError {
    SkillStoreError::Io(format!("{what}: {e}"))
}

pub struct SkillStore {
    host: Host,
    root: PathBuf,
    /// Held across a write or a delete so the check of who made a skill and the change that follows it cannot be split
    /// by another request for the same skill; there is one plugin process, so a lock here is enough.
    write_lock: Mutex<()>,
}

impl SkillStore {
    pub fn new(host: Host, root: PathBuf) -> Self {
        Self { host, root, write_lock: Mutex::new(()) }
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

    async fn row(&self, user_id: i64, name: &str) -> Result<Option<Row>, SkillStoreError> {
        let rows = self
            .host
            .db_query("SELECT id, name, description, origin, enabled FROM items WHERE user_id = $1 AND name = $2", &[json!(user_id), json!(name)])
            .await?;
        rows.first().map(Row::of).transpose()
    }

    pub async fn list(&self, user_id: i64) -> Result<Vec<Skill>, SkillStoreError> {
        let rows = self
            .host
            .db_query("SELECT id, name, description, origin, enabled FROM items WHERE user_id = $1 ORDER BY name", &[json!(user_id)])
            .await?;
        rows.iter().map(|row| Row::of(row).map(|r| r.skill)).collect()
    }

    /// A skill the model may use: one that is turned off is not found, as if it were not there.
    pub async fn get_enabled(&self, user_id: i64, name: &str) -> Result<Skill, SkillStoreError> {
        let skill = self.get(user_id, name).await?;
        if skill.enabled { Ok(skill) } else { Err(SkillStoreError::NotFound(format!("skill '{name}'"))) }
    }

    pub async fn get(&self, user_id: i64, name: &str) -> Result<Skill, SkillStoreError> {
        self.skill_dir(user_id, name)?;
        self.row(user_id, name).await?.map(|row| row.skill).ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))
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
        let real = tokio::fs::canonicalize(&wanted).await.map_err(|_| SkillStoreError::NotFound(format!("file '{path}' in skill '{name}'")))?;
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

    /// Turns a skill on or off for the model, if `by` may change it (the model, only a skill it made that is still on, which
    /// is to say it can only turn it off). The files are not touched.
    pub async fn set_enabled(&self, user_id: i64, name: &str, enabled: bool, by: Origin) -> Result<(), SkillStoreError> {
        self.skill_dir(user_id, name)?;
        let _guard = self.write_lock.lock().await;
        let existing = self.row(user_id, name).await?.ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))?;
        Self::allowed(&existing.skill, by, name)?;
        self.host.db_execute("UPDATE items SET enabled = $1 WHERE id = $2", &[json!(enabled), json!(existing.id)]).await?;
        Ok(())
    }

    /// Whether `by` may change `existing`.
    fn allowed(existing: &Skill, by: Origin, name: &str) -> Result<(), SkillStoreError> {
        if by == Origin::Model && !existing.enabled {
            return Err(SkillStoreError::Refused(format!(
                "skill '{name}' is turned off by the user, so only the user can change it; use another name"
            )));
        }
        if by == Origin::Model && existing.origin == Origin::User {
            return Err(SkillStoreError::Refused(format!(
                "skill '{name}' was added by the user, so only the user can change it; use another name"
            )));
        }
        Ok(())
    }

    /// Stores a skill, replacing one of the same name `by` is allowed to change. `files` are named from the skill's own
    /// folder and must hold the `SKILL.md`; the skill's folder is built beside the old one and swapped in, so a failure
    /// part-way leaves the old skill as it was.
    pub async fn save(&self, user_id: i64, name: &str, description: &str, files: Vec<(String, Vec<u8>)>, by: Origin) -> Result<Skill, SkillStoreError> {
        let dir = self.skill_dir(user_id, name)?;
        let files = checked_files(files)?;
        // One line wherever it is shown: the prompt lists skills a line each
        let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
        if description.is_empty() {
            return Err(SkillStoreError::Invalid("a skill needs a description that says when to use it".to_string()));
        }

        let _guard = self.write_lock.lock().await;
        if let Some(existing) = self.row(user_id, name).await? {
            Self::allowed(&existing.skill, by, name)?;
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

        // A replaced skill keeps whether the user has it switched on, as the row's `enabled` is left alone on a conflict
        let saved = self
            .host
            .db_execute(
                "INSERT INTO items (user_id, name, description, origin, enabled) VALUES ($1, $2, $3, $4, TRUE) \
                 ON CONFLICT (user_id, name) DO UPDATE SET description = EXCLUDED.description, origin = EXCLUDED.origin, updated_at = now()",
                &[json!(user_id), json!(name), json!(description), json!(by.as_str())],
            )
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
        let existing = self.row(user_id, name).await?.ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))?;
        Self::allowed(&existing.skill, by, name)?;

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

        self.host.db_execute("UPDATE items SET updated_at = now() WHERE id = $1", &[json!(existing.id)]).await?;
        Ok(())
    }

    /// Removes every file of a user's skills, for when the user was deleted (their rows went with them through the
    /// foreign key). Nothing is left to ask who made what, so the whole folder goes.
    pub async fn remove_user_files(&self, user_id: i64) -> Result<(), SkillStoreError> {
        let _guard = self.write_lock.lock().await;
        match tokio::fs::remove_dir_all(self.user_dir(user_id)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io("could not remove the user's skills", e)),
        }
    }

    /// Deletes a skill `by` is allowed to change: the row first (so it drops out of the prompt at once), then the folder.
    pub async fn delete(&self, user_id: i64, name: &str, by: Origin) -> Result<(), SkillStoreError> {
        let dir = self.skill_dir(user_id, name)?;
        let _guard = self.write_lock.lock().await;
        let existing = self.row(user_id, name).await?.ok_or_else(|| SkillStoreError::NotFound(format!("skill '{name}'")))?;
        Self::allowed(&existing.skill, by, name)?;
        self.host.db_execute("DELETE FROM items WHERE id = $1", &[json!(existing.id)]).await?;
        // The row is gone, so a folder that cannot be removed now is only wasted space, not a skill that still shows
        if let Err(e) = tokio::fs::remove_dir_all(&dir).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("skill folder {} could not be removed: {e}", dir.display());
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
