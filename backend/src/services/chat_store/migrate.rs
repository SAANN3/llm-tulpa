use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// Ensures the schema exists — idempotent `CREATE TABLE`/`CREATE INDEX` statements run
/// on every boot, no migration history table or rollback support. Deliberately chosen
/// over `sea-orm-migration`: one schema, one owner, no need yet for versioned/rollback
/// migrations. Revisit if the schema starts changing often enough that "what changed
/// and when" becomes something worth tracking.
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "
        CREATE TABLE IF NOT EXISTS chats (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            is_deleted BOOLEAN NOT NULL DEFAULT false,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE INDEX IF NOT EXISTS idx_chats_is_deleted_updated_at
            ON chats (is_deleted, updated_at);

        -- Rolling compaction summary: `summary` covers every message up to and
        -- including `summary_up_to_message_id`; `ollama_history` sends this in place of
        -- that older stretch of raw history once it's set. Both null until a chat's
        -- history first crosses the compaction threshold.
        ALTER TABLE chats ADD COLUMN IF NOT EXISTS summary TEXT;
        ALTER TABLE chats ADD COLUMN IF NOT EXISTS summary_up_to_message_id BIGINT;

        CREATE TABLE IF NOT EXISTS messages (
            id BIGSERIAL PRIMARY KEY,
            chat_id BIGINT NOT NULL REFERENCES chats (id),
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_name TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE INDEX IF NOT EXISTS idx_messages_chat_id_created_at
            ON messages (chat_id, created_at);

        -- `CREATE TABLE IF NOT EXISTS` above is a no-op once the table already exists,
        -- so columns added after the table's first deploy need their own idempotent
        -- statement here rather than editing the `CREATE TABLE` block itself.
        ALTER TABLE messages ADD COLUMN IF NOT EXISTS thinking TEXT;
        ALTER TABLE messages ADD COLUMN IF NOT EXISTS thought_duration_ms BIGINT;
        ALTER TABLE messages ADD COLUMN IF NOT EXISTS tool_success BOOLEAN;
        ALTER TABLE messages ADD COLUMN IF NOT EXISTS tool_denied BOOLEAN NOT NULL DEFAULT false;

        CREATE TABLE IF NOT EXISTS tool_calls (
            id BIGSERIAL PRIMARY KEY,
            message_id BIGINT NOT NULL REFERENCES messages (id),
            tool_name TEXT NOT NULL,
            arguments JSONB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tool_calls_message_id
            ON tool_calls (message_id);
        ",
    )
    .await?;

    Ok(())
}
