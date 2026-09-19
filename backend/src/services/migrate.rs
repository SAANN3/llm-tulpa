use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Statement};

/// Bumped whenever the schema below changes. On a version mismatch (including the first
/// run of this multi-user schema against the old single-user database) every known table
/// is dropped and recreated — this project deliberately wipes rather than writing
/// data-preserving migrations (see the plan / AGENTS notes). Replaces the five scattered
/// per-store `migrate.rs` files with one ordered, centralized migration.
const SCHEMA_VERSION: i32 = 2;

/// Ensures the 3NF schema exists at `SCHEMA_VERSION`. Idempotent: on a matching version it
/// is a no-op; otherwise it drops all known tables (old single-user names included) and
/// recreates the current schema, then records the version. Run once at bootstrap against a
/// connection already pointed at the target database.
pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS schema_meta (
            id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
            version INT NOT NULL
        );",
    )
    .await?;

    let current: Option<i32> = db
        .query_one_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT version FROM schema_meta WHERE id = TRUE",
        ))
        .await?
        .map(|row| row.try_get("", "version"))
        .transpose()?;

    if current == Some(SCHEMA_VERSION) {
        return Ok(());
    }

    // Version mismatch or fresh database → wipe every table this app has ever created
    // (old and new names) and rebuild. CASCADE makes drop order irrelevant.
    db.execute_unprepared(
        "
        DROP TABLE IF EXISTS
            user_plugins, tool_permissions, message_files, message_images, tool_calls,
            messages, plugin_chats, files, chats, user_settings, llm_models, llm_providers,
            users, plugin_settings, settings
        CASCADE;
        ",
    )
    .await?;

    db.execute_unprepared(
        "
        CREATE TABLE users (
            id BIGSERIAL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            CONSTRAINT users_role_valid CHECK (role IN ('owner', 'user'))
        );

        -- LLM backends the app can talk to. Seeded below; a model's provider is reached
        -- through llm_models, never duplicated onto the rows that reference a model.
        CREATE TABLE llm_providers (
            id BIGSERIAL PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        );
        INSERT INTO llm_providers (name) VALUES ('ollama');

        -- Models known to the app, registered when a user picks or pulls one.
        CREATE TABLE llm_models (
            id BIGSERIAL PRIMARY KEY,
            provider_id BIGINT NOT NULL REFERENCES llm_providers (id),
            name TEXT NOT NULL,
            CONSTRAINT llm_models_provider_name_unique UNIQUE (provider_id, name)
        );

        -- Per-user preferences, 1:1 with users (replaces the old global singleton row).
        CREATE TABLE user_settings (
            user_id BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
            name TEXT,
            timezone SMALLINT,
            notifications_enabled BOOLEAN NOT NULL DEFAULT false,
            theme TEXT,
            language TEXT NOT NULL DEFAULT 'en',
            active_model_id BIGINT REFERENCES llm_models (id) ON DELETE SET NULL
        );

        -- Every chat is bound to exactly one model for its whole life (switchable, never
        -- absent). RESTRICT: a model still referenced by a chat can't be removed.
        CREATE TABLE chats (
            id BIGSERIAL PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            model_id BIGINT NOT NULL REFERENCES llm_models (id) ON DELETE RESTRICT,
            is_deleted BOOLEAN NOT NULL DEFAULT false,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            summary TEXT,
            summary_up_to_message_id BIGINT
        );
        CREATE INDEX idx_chats_user_deleted_updated ON chats (user_id, is_deleted, updated_at);

        -- The plugin-owned-chat mapping, normalized out of chats' old nullable triple.
        CREATE TABLE plugin_chats (
            chat_id BIGINT PRIMARY KEY REFERENCES chats (id) ON DELETE CASCADE,
            plugin_name TEXT NOT NULL,
            plugin_subname TEXT NOT NULL,
            plugin_chat_id TEXT NOT NULL,
            CONSTRAINT plugin_chats_unique UNIQUE (plugin_name, plugin_subname, plugin_chat_id)
        );

        CREATE TABLE messages (
            id BIGSERIAL PRIMARY KEY,
            chat_id BIGINT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_name TEXT,
            thinking TEXT,
            thought_duration_ms BIGINT,
            tool_success BOOLEAN,
            tool_denied BOOLEAN NOT NULL DEFAULT false,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        CREATE INDEX idx_messages_chat_created ON messages (chat_id, created_at);

        CREATE TABLE files (
            id BIGSERIAL PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
            chat_id BIGINT REFERENCES chats (id) ON DELETE SET NULL,
            full_path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            read_only BOOLEAN NOT NULL DEFAULT TRUE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            CONSTRAINT files_full_path_unique UNIQUE (full_path)
        );
        CREATE INDEX idx_files_user ON files (user_id);
        CREATE INDEX idx_files_chat ON files (chat_id);

        -- Base64 images attached to a message, normalized out of the old JSONB array.
        CREATE TABLE message_images (
            id BIGSERIAL PRIMARY KEY,
            message_id BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
            position INT NOT NULL,
            data TEXT NOT NULL
        );
        CREATE INDEX idx_message_images_message ON message_images (message_id);

        -- Files attached to a message, normalized out of the old JSONB id array.
        CREATE TABLE message_files (
            message_id BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
            file_id BIGINT NOT NULL REFERENCES files (id) ON DELETE CASCADE,
            position INT NOT NULL,
            PRIMARY KEY (message_id, file_id)
        );

        CREATE TABLE tool_calls (
            id BIGSERIAL PRIMARY KEY,
            message_id BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
            tool_name TEXT NOT NULL,
            arguments JSONB NOT NULL,
            position INT NOT NULL DEFAULT 0
        );
        CREATE INDEX idx_tool_calls_message ON tool_calls (message_id);

        -- Scoped by chat (which carries user_id); deliberately no user_id here — that
        -- would be a transitive dependency through chat_id and violate 3NF.
        CREATE TABLE tool_permissions (
            id BIGSERIAL PRIMARY KEY,
            chat_id BIGINT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
            tool_name TEXT NOT NULL,
            scope JSONB NOT NULL,
            CONSTRAINT tool_permissions_chat_tool_unique UNIQUE (chat_id, tool_name)
        );
        CREATE INDEX idx_tool_permissions_chat ON tool_permissions (chat_id);

        CREATE TABLE user_plugins (
            id BIGSERIAL PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
            plugin_name TEXT NOT NULL,
            plugin_subname TEXT NOT NULL,
            settings JSONB NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT false,
            CONSTRAINT user_plugins_unique UNIQUE (user_id, plugin_name, plugin_subname)
        );
        ",
    )
    .await?;

    db.execute_unprepared(&format!(
        "INSERT INTO schema_meta (id, version) VALUES (TRUE, {SCHEMA_VERSION})
         ON CONFLICT (id) DO UPDATE SET version = {SCHEMA_VERSION};"
    ))
    .await?;

    Ok(())
}
