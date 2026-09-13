use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// Ensures the schema exists — idempotent `CREATE TABLE` run on every boot, same
/// deliberate no-migration-history approach as `chat_store::migrate`. `full_path` is
/// unique: it's the on-disk filename `store_bytes` generates (timestamp + chat id), so
/// a collision would mean two rows silently pointing at the same file.
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "
        CREATE TABLE IF NOT EXISTS files (
            id BIGSERIAL PRIMARY KEY,
            chat_id BIGINT REFERENCES chats (id),
            full_path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            read_only BOOLEAN NOT NULL DEFAULT TRUE,
            CONSTRAINT files_full_path_unique UNIQUE (full_path)
        );

        -- `chat_id` started out NOT NULL; this drops that for anyone whose `files`
        -- table already existed before it did (a no-op, not an error, once it's
        -- already nullable). NULL means \"uploaded before a chat existed yet\" — the
        -- home page's case, where a file needs to be storable before the chat it'll
        -- end up on has been created. `Agent::chat` claims it (sets a real `chat_id`)
        -- the moment it actually gets attached to a message.
        ALTER TABLE files ALTER COLUMN chat_id DROP NOT NULL;

        CREATE INDEX IF NOT EXISTS idx_files_chat_id
            ON files (chat_id);
        ",
    )
    .await?;

    Ok(())
}
