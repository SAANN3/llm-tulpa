use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Statement};

/// Bumped whenever a change makes previously-stored `tool_permissions` rows meaningless
/// under the new scheme (not just a new column) — see the `2` bump below for why. Stored
/// in `permission_schema_meta`, a one-row table private to this migration.
const SCHEMA_VERSION: i32 = 2;

/// Ensures the schema exists — idempotent `CREATE TABLE` run on every boot, same
/// deliberate no-migration-history approach as `chat_store::migrate`. The unique
/// constraint on `(chat_id, tool_name)` is what makes `update_scope`'s upsert
/// well-defined — at most one grant per tool per chat.
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "
        CREATE TABLE IF NOT EXISTS tool_permissions (
            id BIGSERIAL PRIMARY KEY,
            chat_id BIGINT NOT NULL REFERENCES chats (id),
            tool_name TEXT NOT NULL,
            scope JSONB NOT NULL,
            CONSTRAINT tool_permissions_chat_tool_unique UNIQUE (chat_id, tool_name)
        );

        CREATE INDEX IF NOT EXISTS idx_tool_permissions_chat_id
            ON tool_permissions (chat_id);

        CREATE TABLE IF NOT EXISTS permission_schema_meta (
            id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
            version INT NOT NULL
        );
        ",
    )
    .await?;

    reset_on_version_change(db).await?;

    Ok(())
}

/// Grants stored under the old per-tool-only scheme (version 1, implicit — the column
/// existed before this versioning did) don't mean anything under the shared
/// read/write/delete buckets this version introduces: a `tool_name` like
/// `storage.read_file` no longer gets checked directly, `GLOBAL.STORAGE_READ` does. Old
/// rows would just sit there unused rather than actively wrong, but wiping them is
/// simpler than reasoning about stale data — and this runs for *any* database that
/// still reports an older version, this user's included, not just future clones. Every
/// grant this drops just means one re-approval prompt next time, nothing lost.
async fn reset_on_version_change(db: &DatabaseConnection) -> Result<(), DbErr> {
    let current: Option<i32> = db
        .query_one_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT version FROM permission_schema_meta WHERE id = TRUE",
        ))
        .await?
        .map(|row| row.try_get("", "version"))
        .transpose()?;

    if current == Some(SCHEMA_VERSION) {
        return Ok(());
    }

    db.execute_unprepared("TRUNCATE TABLE tool_permissions;").await?;
    db.execute_raw(Statement::from_sql_and_values(
        db.get_database_backend(),
        "INSERT INTO permission_schema_meta (id, version) VALUES (TRUE, $1) \
         ON CONFLICT (id) DO UPDATE SET version = EXCLUDED.version",
        [SCHEMA_VERSION.into()],
    ))
    .await?;

    Ok(())
}
