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
    /// The plugin's own schema and role need a name Postgres can hold (63 bytes)
    NameTooLong(String),
    /// The database user may not create roles, which a plugin with a schema of its own needs
    NoCreateRole,
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
            PluginSchemaError::NameTooLong(name) => write!(f, "'{name}' is longer than the 63 bytes Postgres allows for a schema name; use a shorter plugin name"),
            PluginSchemaError::NoCreateRole => write!(f, "the database user cannot create roles (CREATEROLE), which a plugin with a database of its own needs"),
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

/// The name of the schema and of the role a plugin with a database of its own gets: the same as its table prefix, so a role,
/// a schema and a ledger entry are all found by one name.
pub fn role_name(plugin_name: &str, plugin_subname: &str) -> Result<String, PluginSchemaError> {
    let name = prefix(plugin_name, plugin_subname)?;
    if name.len() > 63 {
        return Err(PluginSchemaError::NameTooLong(name));
    }
    Ok(name)
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

    /// Makes the schema and the role of a plugin with a database of its own, if they are not there yet. The role cannot log
    /// in and owns the schema; it may only reference the user ids of the core tables, so that `REFERENCES users (id)` works in
    /// its tables. The backend's own user is made a member of it, which is what lets `SET ROLE` to it work (a superuser needs
    /// no membership, and it is harmless there).
    pub async fn prepare_own_schema(&self, plugin_name: &str, plugin_subname: &str) -> Result<String, PluginSchemaError> {
        let role = role_name(plugin_name, plugin_subname)?;
        let allowed = self
            .db
            .query_one_raw(Statement::from_string(DbBackend::Postgres, "SELECT rolcreaterole OR rolsuper AS ok FROM pg_roles WHERE rolname = current_user"))
            .await?
            .map(|row| row.try_get::<bool>("", "ok"))
            .transpose()?
            .unwrap_or(false);
        if !allowed {
            return Err(PluginSchemaError::NoCreateRole);
        }
        // `role` is a plain lowercase word (checked by `prefix`), so it can be written into the statements. One transaction:
        // a failure half way leaves no role without its schema.
        let txn = self.db.begin().await?;
        txn.execute_unprepared(&format!(
            "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{role}') THEN CREATE ROLE {role} NOLOGIN; END IF; END $$"
        ))
        .await?;
        txn.execute_unprepared(&format!("GRANT {role} TO CURRENT_USER")).await?;
        txn.execute_unprepared(&format!("CREATE SCHEMA IF NOT EXISTS {role} AUTHORIZATION {role}")).await?;
        txn.execute_unprepared(&format!("GRANT REFERENCES (id) ON TABLE users TO {role}")).await?;
        txn.commit().await?;
        Ok(role)
    }

    /// `apply` for a plugin with a schema of its own (`prepare_own_schema` has run): each step runs as the plugin's role with
    /// its schema first on the search path, so a plain `CREATE TABLE items` lands in the plugin's schema and a statement
    /// that reaches for a core table is refused by Postgres for want of a privilege. The same rules apply as for any plugin's
    /// steps, except that names need no prefix: the schema is the plugin's own.
    pub async fn apply_in_own_schema(&self, plugin_name: &str, plugin_subname: &str, steps: &[String]) -> Result<usize, PluginSchemaError> {
        let role = role_name(plugin_name, plugin_subname)?;
        check_steps("", steps)?;
        let applied = self.applied(plugin_name, plugin_subname).await?;
        if applied > steps.len() {
            return Err(PluginSchemaError::Ahead { applied, known: steps.len() });
        }
        for (at, step) in steps.iter().enumerate().skip(applied) {
            let txn = self.db.begin().await?;
            txn.execute_unprepared(&format!("SET LOCAL ROLE {role}")).await?;
            txn.execute_unprepared(&format!("SET LOCAL search_path = {role}, public")).await?;
            txn.execute_unprepared(step).await?;
            // The ledger is the backend's, not the plugin's
            txn.execute_unprepared("RESET ROLE").await?;
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
    fn a_plugin_with_a_schema_of_its_own_has_a_role_of_the_same_name_that_fits_postgres() {
        assert_eq!(role_name("example", "skills").unwrap(), "plugin_example_skills");
        assert!(matches!(role_name(&"n".repeat(40), &"s".repeat(30)), Err(PluginSchemaError::NameTooLong(_))));
        assert!(matches!(role_name("Bad", "x"), Err(PluginSchemaError::BadName(_))));
    }

    #[test]
    fn in_a_schema_of_its_own_names_need_no_prefix_but_a_plugin_still_only_adds() {
        let steps = |s: &str| vec![s.to_string()];
        assert!(check_steps("", &steps("CREATE TABLE items (id int)")).is_ok());
        assert!(check_steps("", &steps("ALTER TABLE items ADD COLUMN note TEXT")).is_ok());
        assert!(check_steps("", &steps("DROP TABLE items")).is_err());
        assert!(check_steps("", &steps("DELETE FROM items")).is_err());
        assert!(check_steps("", &steps("CREATE TABLE \"Items\" (id int)")).is_err());
        assert!(check_steps("", &steps("SELECT 1")).is_err());
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
