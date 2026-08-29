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

        -- Tags a chat as owned by a plugin instead of created directly by the user —
        -- e.g. a Telegram conversation the messaging plugin is relaying through the
        -- agent. All three null (an ordinary chat) or all three filled; enforced below,
        -- not just by convention. `plugin_chat_id` is the *provider's own* identifier
        -- for the conversation (a Telegram chat id, a Discord channel id, ...) — opaque
        -- to everything except that one plugin instance, and is what a plugin looks up
        -- an incoming message's chat by before falling back to creating a new one.
        ALTER TABLE chats ADD COLUMN IF NOT EXISTS plugin_name TEXT;
        ALTER TABLE chats ADD COLUMN IF NOT EXISTS plugin_subname TEXT;
        ALTER TABLE chats ADD COLUMN IF NOT EXISTS plugin_chat_id TEXT;

        -- Postgres has no `ADD CONSTRAINT IF NOT EXISTS` (unlike `ADD COLUMN` above),
        -- so re-running this idempotently on every boot means catching the
        -- already-exists error instead. `duplicate_object` is specifically what
        -- Postgres raises for a constraint name that's already taken.
        DO $$ BEGIN
            ALTER TABLE chats ADD CONSTRAINT chats_plugin_all_or_none CHECK (
                (plugin_name IS NULL) = (plugin_subname IS NULL) AND
                (plugin_name IS NULL) = (plugin_chat_id IS NULL)
            );
        EXCEPTION WHEN duplicate_object THEN NULL;
        END $$;

        -- At most one chat per (plugin, subplugin, external chat) triple — this is what
        -- makes an incoming-message handler's \"find or create\" safe to rely on.
        -- Postgres's standard multi-column UNIQUE semantics already do the right thing
        -- for ordinary chats (all three NULL): a row with a NULL in any of these
        -- columns is never considered a duplicate of another such row, so this doesn't
        -- also need a partial index or `NULLS NOT DISTINCT` to allow many plugin-less
        -- chats to coexist.
        -- Unlike the CHECK constraint above, a UNIQUE constraint implicitly creates a
        -- same-named backing index — re-adding an already-existing one fails while
        -- Postgres tries to recreate *that* index, which raises `duplicate_table`
        -- (\"relation already exists\"), not `duplicate_object`. Both need catching here.
        DO $$ BEGIN
            ALTER TABLE chats ADD CONSTRAINT chats_plugin_chat_unique
                UNIQUE (plugin_name, plugin_subname, plugin_chat_id);
        EXCEPTION
            WHEN duplicate_object THEN NULL;
            WHEN duplicate_table THEN NULL;
        END $$;

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
        -- Base64-encoded image data (no data-URL prefix), one entry per attached image.
        -- Only ever set on `user`-role messages. Null rather than `[]` when a message
        -- has none, same convention as `summary`/`thinking` above.
        ALTER TABLE messages ADD COLUMN IF NOT EXISTS images JSONB;

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
