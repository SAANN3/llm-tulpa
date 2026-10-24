//! The database calls of a plugin (`db.query`, `db.execute`, `db.batch`). They run on the backend's pool inside a
//! transaction that has switched to the plugin's role with its schema first on the search path, so a plain table name is the
//! plugin's own table and a core table is refused for want of a privilege.
//!
//! That is a guard against mistakes, not against a plugin that means harm: the plugin is a program the owner installed and
//! trusts, it runs as the owner and can read the backend's settings file, and a statement of its own (`RESET ROLE`) leaves the
//! role again. The trust model is the one in `plugin-sdk/PROTOCOL.md`.

use std::time::Duration;

use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, RuntimeErr, Statement, TransactionTrait};
use serde::Deserialize;
use serde_json::Value;

/// A statement running longer than this is cancelled by Postgres, a little before the call's own time limit so the plugin
/// gets the database's reason and not just silence
const STATEMENT_TIMEOUT: Duration = Duration::from_secs(25);
/// Rows are sent to the plugin in one protocol line, which has a size limit; a result larger than this asks for a LIMIT
const MAX_RESULT_BYTES: usize = 8 * 1024 * 1024;
/// Statements in one `db.batch`
const MAX_BATCH: usize = 1000;

/// One statement of a call: the SQL with `$1`, `$2`... and the values for them
#[derive(Debug, Deserialize)]
pub struct Sql {
    pub sql: String,
    #[serde(default)]
    pub params: Vec<Value>,
}

pub struct PluginDb {
    db: DatabaseConnection,
    role: String,
}

impl PluginDb {
    /// `role` is the plugin's schema and role (`schema::role_name`), a plain lowercase word, so it can be written into SQL.
    pub fn new(db: DatabaseConnection, role: String) -> Self {
        Self { db, role }
    }

    /// Runs a statement that returns rows (a SELECT, or an INSERT/UPDATE/DELETE with RETURNING) and gives them as a list of
    /// objects, one per row, keyed by column name.
    pub async fn query(&self, statement: Sql) -> Result<Value, String> {
        // Postgres turns the rows into JSON itself, so every column type has a JSON form without this module knowing it. The
        // statement is a CTE, which is what lets a data-changing statement with RETURNING be asked for its rows too.
        let wrapped = format!(
            "WITH q AS ({}) SELECT coalesce(json_agg(q), '[]'::json)::text AS rows FROM q",
            statement.sql.trim().trim_end_matches(';')
        );
        let txn = self.scoped().await.map_err(explain)?;
        let row = txn.query_one_raw(bound(&wrapped, &statement.params)?).await.map_err(explain)?;
        txn.commit().await.map_err(explain)?;
        let text = row.ok_or("the query gave no result")?.try_get::<String>("", "rows").map_err(explain)?;
        if text.len() > MAX_RESULT_BYTES {
            return Err(format!("the result is {} MiB, more than the {} MiB a call may return; add a LIMIT", text.len() / (1024 * 1024), MAX_RESULT_BYTES / (1024 * 1024)));
        }
        serde_json::from_str(&text).map_err(|e| format!("the rows were not JSON: {e}"))
    }

    /// Runs a statement and gives the number of rows it changed
    pub async fn execute(&self, statement: Sql) -> Result<u64, String> {
        let txn = self.scoped().await.map_err(explain)?;
        let done = txn.execute_raw(bound(&statement.sql, &statement.params)?).await.map_err(explain)?;
        txn.commit().await.map_err(explain)?;
        Ok(done.rows_affected())
    }

    /// Runs the statements in one transaction, all or none, and gives how many rows each changed
    pub async fn batch(&self, statements: Vec<Sql>) -> Result<Vec<u64>, String> {
        if statements.len() > MAX_BATCH {
            return Err(format!("a batch takes at most {MAX_BATCH} statements"));
        }
        let txn = self.scoped().await.map_err(explain)?;
        let mut counts = Vec::with_capacity(statements.len());
        for (at, statement) in statements.iter().enumerate() {
            let done = txn.execute_raw(bound(&statement.sql, &statement.params)?).await.map_err(|e| format!("statement {}: {}", at + 1, explain(e)))?;
            counts.push(done.rows_affected());
        }
        txn.commit().await.map_err(explain)?;
        Ok(counts)
    }

    async fn scoped(&self) -> Result<DatabaseTransaction, DbErr> {
        let txn = self.db.begin().await?;
        txn.execute_unprepared(&format!("SET LOCAL ROLE {}", self.role)).await?;
        txn.execute_unprepared(&format!("SET LOCAL search_path = {}, public", self.role)).await?;
        txn.execute_unprepared(&format!("SET LOCAL statement_timeout = {}", STATEMENT_TIMEOUT.as_millis())).await?;
        Ok(txn)
    }
}

/// The statement with its values. A value goes to Postgres as the type its JSON form has: text, bigint, double precision,
/// boolean, or jsonb for an array or object. A column of another type (a timestamp, a uuid) needs a cast in the SQL
/// (`$1::timestamptz`), and so does a `null`, which arrives as text.
fn bound(sql: &str, params: &[Value]) -> Result<Statement, String> {
    let values = params
        .iter()
        .enumerate()
        .map(|(at, param)| -> Result<sea_orm::Value, String> {
            Ok(match param {
                Value::Null => Option::<String>::None.into(),
                Value::Bool(b) => (*b).into(),
                Value::String(s) => s.clone().into(),
                Value::Number(n) => match (n.as_i64(), n.as_f64()) {
                    (Some(i), _) => i.into(),
                    // An integer too large for bigint is an error, not a silent float
                    (None, _) if n.is_u64() => return Err(format!("parameter ${}: {n} does not fit in a bigint", at + 1)),
                    (None, Some(f)) => f.into(),
                    (None, None) => return Err(format!("parameter ${}: {n} is not a usable number", at + 1)),
                },
                other => other.clone().into(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Statement::from_sql_and_values(DbBackend::Postgres, sql, values))
}

/// What Postgres said, with its error code (`23505` for a duplicate key), in place of the driver's wrapping of it
fn explain(e: DbErr) -> String {
    match &e {
        DbErr::Query(RuntimeErr::SqlxError(err)) | DbErr::Exec(RuntimeErr::SqlxError(err)) => match err.as_database_error() {
            Some(db) => match db.code() {
                Some(code) => format!("{} (SQLSTATE {code})", db.message()),
                None => db.message().to_string(),
            },
            None => e.to_string(),
        },
        _ => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_go_to_postgres_as_the_type_their_json_has() {
        let statement = bound("SELECT $1, $2, $3, $4, $5, $6", &[json!("a"), json!(7), json!(1.5), json!(true), json!(null), json!([1, 2])]).unwrap();
        let values = statement.values.unwrap().0;
        assert!(matches!(values[0], sea_orm::Value::String(Some(_))));
        assert!(matches!(values[1], sea_orm::Value::BigInt(Some(7))));
        assert!(matches!(values[2], sea_orm::Value::Double(Some(_))));
        assert!(matches!(values[3], sea_orm::Value::Bool(Some(true))));
        assert!(matches!(values[4], sea_orm::Value::String(None)));
        assert!(matches!(values[5], sea_orm::Value::Json(Some(_))));
    }

    #[test]
    fn an_integer_too_large_for_bigint_is_refused() {
        let err = bound("SELECT $1", &[json!(u64::MAX)]).unwrap_err();
        assert!(err.contains("$1") && err.contains("bigint"), "{err}");
    }
}
