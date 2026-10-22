//! The tables a plugin declares for itself. A plugin builder returns its SQL as ordered steps (`PluginBuilder::migrations`);
//! the registry applies the ones a database has not seen yet before the plugin is registered, so a plugin that needs a table
//! brings it along instead of the core schema growing for it.
//!
//! This is the one place besides `services::migrate` and `bootstrap` that runs raw SQL: a plugin's steps are DDL by nature. What keeps
//! it safe is that the registry hands the plugin a prefix and refuses any step that touches a table or index outside it, or
//! that drops, truncates or deletes anything: a plugin can add to the schema and to its own tables, never remove, and
//! never reach the core tables (a foreign key to one, `REFERENCES users (id)`, is fine). Steps are applied in order, each in
//! its own transaction together with the plugin's entry in `plugin_migrations`, so a step that fails leaves the schema and
//! the ledger as they were, and a step already applied is never run again. A released step must never be edited or
//! reordered: the ledger only counts how many have been applied.

use std::fmt;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement, TransactionTrait};

/// Owns the ledger of applied plugin migrations (table `plugin_migrations`, created by the core schema) and runs the steps.
pub struct PluginSchema {
    db: DatabaseConnection,
}

#[derive(Debug)]
pub enum PluginSchemaError {
    /// The plugin's name cannot be turned into a table prefix
    BadName(String),
    /// A step breaks the rules above; which one, and why
    Refused { step: usize, why: String },
    /// The database has applied more steps than the build has: it is newer than this build, refused like a newer schema
    Ahead { applied: usize, known: usize },
    Query(DbErr),
}

impl fmt::Display for PluginSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginSchemaError::BadName(name) => write!(f, "'{name}' cannot be used in a table name; use lowercase letters, digits and underscores"),
            PluginSchemaError::Refused { step, why } => write!(f, "database step {step} is refused: {why}"),
            PluginSchemaError::Ahead { applied, known } => {
                write!(f, "the database has {applied} steps of this plugin applied but this build knows {known}; use a matching build")
            }
            PluginSchemaError::Query(e) => write!(f, "database error: {e}"),
        }
    }
}

impl From<DbErr> for PluginSchemaError {
    fn from(e: DbErr) -> Self {
        PluginSchemaError::Query(e)
    }
}

/// The prefix every table and index of a plugin starts with: `plugin_<name>_<subname>`. Names with anything but lowercase
/// letters, digits and underscores (a hyphen, say) are refused rather than guessed at.
pub fn prefix(plugin_name: &str, plugin_subname: &str) -> Result<String, PluginSchemaError> {
    for part in [plugin_name, plugin_subname] {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(PluginSchemaError::BadName(part.to_string()));
        }
    }
    Ok(format!("plugin_{plugin_name}_{plugin_subname}"))
}

/// The words of a statement, upper-cased, with `ON DELETE` taken out (a foreign key's `ON DELETE CASCADE` is no deletion).
fn words_of(statement: &str) -> Vec<String> {
    let cleaned: String = statement
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { ' ' })
        .collect();
    let words: Vec<String> = cleaned.split_whitespace().map(|w| w.to_lowercase()).collect();
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        if words[i] == "on" && words.get(i + 1).map(String::as_str) == Some("delete") {
            i += 2;
            continue;
        }
        out.push(words[i].clone());
        i += 1;
    }
    out
}

/// Whether `steps` only add to the plugin's own tables, see the module's comment. `Ok` when every statement is allowed.
pub fn check_steps(prefix: &str, steps: &[String]) -> Result<(), PluginSchemaError> {
    for (at, step) in steps.iter().enumerate() {
        let refuse = |why: String| PluginSchemaError::Refused { step: at + 1, why };
        if step.contains('"') {
            return Err(refuse("quoted names are not allowed".to_string()));
        }
        for statement in step.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let words = words_of(statement);
            let word = |i: usize| words.get(i).map(String::as_str).unwrap_or("");
            if let Some(bad) = words.iter().find(|w| matches!(w.as_str(), "drop" | "truncate" | "delete")) {
                return Err(refuse(format!("`{bad}` is not allowed: a plugin only adds")));
            }
            // The words after the verb: IF NOT EXISTS and the like are skipped to reach the name
            let skip = |mut i: usize, noise: &[&str]| {
                while noise.contains(&word(i)) {
                    i += 1;
                }
                i
            };
            let targets: Vec<&str> = match (word(0), word(1)) {
                ("create", "table") => vec![word(skip(2, &["if", "not", "exists"]))],
                ("create", "index") | ("create", "unique") => {
                    let from = if word(1) == "unique" { 3 } else { 2 };
                    let idx = skip(from, &["if", "not", "exists", "concurrently"]);
                    // ... INDEX <name> ON <table>
                    let on = words.iter().position(|w| w == "on").map(|p| p + 1).map(|p| skip(p, &["only"]));
                    vec![word(idx), on.map(word).unwrap_or("")]
                }
                ("alter", "table") => vec![word(skip(2, &["if", "exists", "only"]))],
                ("insert", "into") => vec![word(2)],
                ("update", _) => vec![word(skip(1, &["only"]))],
                _ => return Err(refuse("only CREATE TABLE, CREATE INDEX, ALTER TABLE, INSERT and UPDATE are allowed".to_string())),
            };
            for target in targets {
                if !target.starts_with(prefix) {
                    return Err(refuse(format!("`{target}` is not a name that starts with `{prefix}`")));
                }
            }
        }
    }
    Ok(())
}

impl PluginSchema {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// How many steps of the plugin the database has applied.
    async fn applied(&self, plugin_name: &str, plugin_subname: &str) -> Result<usize, PluginSchemaError> {
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT version FROM plugin_migrations WHERE plugin_name = $1 AND plugin_subname = $2",
                [plugin_name.into(), plugin_subname.into()],
            ))
            .await?;
        Ok(match row {
            Some(row) => row.try_get::<i32>("", "version")?.max(0) as usize,
            None => 0,
        })
    }

    /// Applies the steps the database has not seen, in order; how many that was. All of `steps` are checked first, so a
    /// refused step is found before anything runs.
    pub async fn apply(&self, plugin_name: &str, plugin_subname: &str, steps: &[String]) -> Result<usize, PluginSchemaError> {
        let prefix = prefix(plugin_name, plugin_subname)?;
        check_steps(&prefix, steps)?;
        let applied = self.applied(plugin_name, plugin_subname).await?;
        if applied > steps.len() {
            return Err(PluginSchemaError::Ahead { applied, known: steps.len() });
        }
        for (at, step) in steps.iter().enumerate().skip(applied) {
            let txn = self.db.begin().await?;
            txn.execute_unprepared(step).await?;
            txn.execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO plugin_migrations (plugin_name, plugin_subname, version) VALUES ($1, $2, $3) \
                 ON CONFLICT (plugin_name, plugin_subname) DO UPDATE SET version = EXCLUDED.version, applied_at = now()",
                [plugin_name.into(), plugin_subname.into(), ((at + 1) as i32).into()],
            ))
            .await?;
            txn.commit().await?;
        }
        Ok(steps.len() - applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "plugin_knowledge_skills";

    fn ok(steps: &[&str]) -> bool {
        check_steps(P, &steps.iter().map(|s| s.to_string()).collect::<Vec<_>>()).is_ok()
    }

    #[test]
    fn the_prefix_is_made_of_the_plugins_name_and_refuses_what_is_not_a_plain_name() {
        assert_eq!(prefix("knowledge", "skills").unwrap(), P);
        assert!(prefix("know-ledge", "skills").is_err());
        assert!(prefix("Knowledge", "skills").is_err());
        assert!(prefix("", "skills").is_err());
        assert!(prefix("a b", "c").is_err());
    }

    #[test]
    fn a_plugin_may_create_and_add_to_its_own_tables() {
        assert!(ok(&["CREATE TABLE plugin_knowledge_skills_items (id BIGSERIAL PRIMARY KEY, user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE, name TEXT NOT NULL)"]));
        assert!(ok(&["create table if not exists plugin_knowledge_skills_items (id int)"]));
        assert!(ok(&["CREATE INDEX plugin_knowledge_skills_items_user ON plugin_knowledge_skills_items (user_id)"]));
        assert!(ok(&["CREATE UNIQUE INDEX IF NOT EXISTS plugin_knowledge_skills_items_name ON plugin_knowledge_skills_items (user_id, name)"]));
        assert!(ok(&["ALTER TABLE plugin_knowledge_skills_items ADD COLUMN origin TEXT NOT NULL DEFAULT 'user'"]));
        assert!(ok(&["UPDATE plugin_knowledge_skills_items SET origin = 'user'", "INSERT INTO plugin_knowledge_skills_items (id) VALUES (1)"]));
        assert!(ok(&["CREATE TABLE plugin_knowledge_skills_a (id int); CREATE TABLE plugin_knowledge_skills_b (id int)"]));
    }

    #[test]
    fn a_step_that_reaches_outside_the_plugin_or_removes_anything_is_refused() {
        assert!(!ok(&["CREATE TABLE skills (id int)"]), "a name without the prefix");
        assert!(!ok(&["CREATE TABLE plugin_other_thing_items (id int)"]), "another plugin's prefix");
        assert!(!ok(&["ALTER TABLE users ADD COLUMN x int"]), "a core table");
        assert!(!ok(&["UPDATE users SET role = 'owner'"]), "a core table's rows");
        assert!(!ok(&["INSERT INTO chats (id) VALUES (1)"]), "a core table's rows");
        assert!(!ok(&["CREATE INDEX plugin_knowledge_skills_x ON users (id)"]), "an index on a core table");
        assert!(!ok(&["CREATE INDEX x_idx ON plugin_knowledge_skills_items (id)"]), "an index with a foreign name");
        assert!(!ok(&["DROP TABLE plugin_knowledge_skills_items"]), "a drop");
        assert!(!ok(&["ALTER TABLE plugin_knowledge_skills_items DROP COLUMN name"]), "a dropped column");
        assert!(!ok(&["TRUNCATE plugin_knowledge_skills_items"]), "a truncate");
        assert!(!ok(&["DELETE FROM plugin_knowledge_skills_items"]), "a delete");
        assert!(!ok(&["CREATE TABLE plugin_knowledge_skills_a (id int); DROP TABLE users"]), "a second statement counts too");
        assert!(!ok(&["CREATE EXTENSION pgcrypto"]), "a verb that is not allowed");
        assert!(!ok(&["CREATE TABLE \"plugin_knowledge_skills_x\" (id int)"]), "quoted names");
        assert!(!ok(&["SELECT 1"]), "a query");
    }
}
