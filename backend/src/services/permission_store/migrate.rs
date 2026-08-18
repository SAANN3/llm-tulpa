use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

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
        ",
    )
    .await?;

    Ok(())
}
