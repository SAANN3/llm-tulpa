use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, Statement, TransactionTrait};

/// The schema version this build creates and expects. Recorded in `schema_meta`.
///
/// Moving to a new version is a data-preserving step, never a wipe: add a
/// `if current < N { ... }` block to `run_migrations` that alters the existing tables in place
/// (inside the same transaction) and bump this constant. A database *newer* than this build is
/// refused rather than "fixed", so an older binary can't damage data a newer one wrote.
const SCHEMA_VERSION: i32 = 3;

/// What version 3 added on top of version 2: the compaction key facts and the background jobs.
/// Run by `create_schema` for a fresh database and by `upgrade_v2_to_v3` for an existing one,
/// so both end up with exactly the same tables. Idempotent.
const V3_ADDITIONS: &str = "
    -- Key facts (structured, append-only): NULL until a chat's first compaction fold.
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS key_facts JSONB;

    -- Background jobs started by `os.start_job`, per chat. Cascades with the chat, so
    -- deleting a user (and with them their chats) isn't blocked by a leftover job row.
    CREATE TABLE IF NOT EXISTS jobs (
        id BIGSERIAL PRIMARY KEY,
        chat_id BIGINT NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
        command TEXT NOT NULL,
        workdir TEXT,
        log_path TEXT NOT NULL DEFAULT '',
        pid BIGINT,
        status TEXT NOT NULL DEFAULT 'running',
        exit_code INTEGER,
        started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        finished_at TIMESTAMPTZ,
        notified BOOLEAN NOT NULL DEFAULT FALSE
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_chat_id ON jobs (chat_id);
";

/// The Postgres schema the pre-accounts (single-user) tables are moved into. See `stash_legacy`.
const LEGACY_SCHEMA: &str = "legacy";

/// The tables the single-user release created, in the `public` schema. Moved as a set.
const LEGACY_TABLES: [&str; 8] = [
    "chats",
    "messages",
    "tool_calls",
    "settings",
    "files",
    "tool_permissions",
    "plugin_settings",
    // Not in the released single-user schema, but a development database from before accounts
    // has it, with a foreign key on `chats` — it has to move along with them.
    "jobs",
];

/// Ensures the schema exists at `SCHEMA_VERSION`. Idempotent: at the current version it does
/// nothing. Run once at bootstrap against a connection already pointed at the target database.
///
/// - **Fresh database** → creates the schema.
/// - **Single-user database** (the release before accounts existed: a `chats` table with no
///   `user_id`) → its tables are *moved*, not dropped, into a `legacy` schema and the new schema
///   is created next to them. There's no user to own that data yet, so it stays there until the
///   owner account is created, at which point `adopt_legacy_data` copies it in.
/// - **Version 2** → upgraded in place (`upgrade_v2_to_v3`), nothing dropped.
/// - **Any other version** → refused with an error; nothing is touched.
///
/// The whole thing runs in one transaction (Postgres DDL is transactional), so a failure leaves
/// the database exactly as it was found.
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
            DbBackend::Postgres,
            "SELECT version FROM schema_meta WHERE id = TRUE",
        ))
        .await?
        .map(|row| row.try_get("", "version"))
        .transpose()?;

    match current {
        Some(SCHEMA_VERSION) => return Ok(()),
        Some(2) => return upgrade_v2_to_v3(db).await,
        Some(other) => {
            return Err(DbErr::Custom(format!(
                "the database is at schema version {other}, but this build understands version {SCHEMA_VERSION}; \
                 refusing to touch it (use a matching build, or a fresh database)"
            )));
        }
        None => {}
    }

    let txn = db.begin().await?;
    if has_single_user_tables(&txn).await? {
        stash_legacy(&txn).await?;
    }
    create_schema(&txn).await?;
    txn.execute_unprepared(&format!(
        "INSERT INTO schema_meta (id, version) VALUES (TRUE, {SCHEMA_VERSION})
         ON CONFLICT (id) DO UPDATE SET version = {SCHEMA_VERSION};"
    ))
    .await?;
    txn.commit().await?;

    Ok(())
}

/// Version 2 → 3, in place: adds `chats.key_facts` and the `jobs` table. One transaction.
async fn upgrade_v2_to_v3(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V3_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// A `chats` table in `public` without a `user_id` column: what the single-user release left.
async fn has_single_user_tables(txn: &DatabaseTransaction) -> Result<bool, DbErr> {
    let row = txn
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT
                EXISTS (SELECT 1 FROM information_schema.tables
                        WHERE table_schema = 'public' AND table_name = 'chats')
                AND NOT EXISTS (SELECT 1 FROM information_schema.columns
                        WHERE table_schema = 'public' AND table_name = 'chats' AND column_name = 'user_id')
                AS legacy",
        ))
        .await?;
    match row {
        Some(row) => row.try_get("", "legacy"),
        None => Ok(false),
    }
}

/// Moves the single-user tables into their own schema. A schema rather than renamed tables
/// because index and constraint names are shared across `public` — a renamed `files` would
/// still own `files_full_path_unique`, which the new `files` table needs for itself. Sequences
/// owned by the tables' columns move with them.
async fn stash_legacy(txn: &DatabaseTransaction) -> Result<(), DbErr> {
    txn.execute_unprepared(&format!("CREATE SCHEMA IF NOT EXISTS {LEGACY_SCHEMA}")).await?;
    for table in LEGACY_TABLES {
        txn.execute_unprepared(&format!("ALTER TABLE IF EXISTS public.{table} SET SCHEMA {LEGACY_SCHEMA}"))
            .await?;
    }
    Ok(())
}

async fn create_schema(txn: &DatabaseTransaction) -> Result<(), DbErr> {
    txn.execute_unprepared(
        "
        CREATE TABLE users (
            id BIGSERIAL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            CONSTRAINT users_role_valid CHECK (role IN ('owner', 'user'))
        );

        -- At most one owner, enforced by the database: two simultaneous first-run requests
        -- can't both create one. `UserStore` recognizes this index by name.
        CREATE UNIQUE INDEX users_one_owner ON users (role) WHERE role = 'owner';

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

        -- Per-user preferences, 1:1 with users. A user's row is created empty on first use.
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
    txn.execute_unprepared(V3_ADDITIONS).await?;
    Ok(())
}

/// Whether a table exists (`schema.table`), via `to_regclass` — which yields NULL, not an
/// error, for one that doesn't.
async fn table_exists(txn: &DatabaseTransaction, qualified: &str) -> Result<bool, DbErr> {
    let row = txn
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT to_regclass($1) IS NOT NULL AS present",
            [qualified.into()],
        ))
        .await?;
    match row {
        Some(row) => row.try_get("", "present"),
        None => Ok(false),
    }
}

/// Whether `schema.table` has a column called `column`.
async fn column_exists(txn: &DatabaseTransaction, schema: &str, table: &str, column: &str) -> Result<bool, DbErr> {
    let row = txn
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                            WHERE table_schema = $1 AND table_name = $2 AND column_name = $3) AS present",
            [schema.into(), table.into(), column.into()],
        ))
        .await?;
    match row {
        Some(row) => row.try_get("", "present"),
        None => Ok(false),
    }
}

/// Copies the data a single-user install left in the `legacy` schema into the current schema,
/// as belonging to `owner_id`, then renames that schema to `legacy_adopted_<unix time>` (kept
/// as a backup — drop it once you're satisfied). Returns whether there was anything to adopt.
///
/// Ids are preserved, so files already on disk and every reference between rows stay valid;
/// the id sequences are moved past the copied rows afterwards. A development database's extras
/// (`chats.key_facts`, the `jobs` table) come over too when they exist. Those installs had no notion of
/// a per-chat model, so every chat is bound to `default_model` (registered under Ollama if it
/// isn't yet, and set as the owner's default) — that's the one model such an install ever ran.
/// All or nothing: it's one transaction.
pub async fn adopt_legacy_data(db: &DatabaseConnection, owner_id: i64, default_model: &str) -> Result<bool, DbErr> {
    let txn = db.begin().await?;

    if !table_exists(&txn, &format!("{LEGACY_SCHEMA}.chats")).await? {
        txn.rollback().await?;
        return Ok(false);
    }

    let owner: [sea_orm::Value; 1] = [owner_id.into()];

    // The model every legacy chat gets bound to, and the owner's default.
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO llm_models (provider_id, name)
         SELECT id, $1 FROM llm_providers WHERE name = 'ollama'
         ON CONFLICT (provider_id, name) DO NOTHING",
        [default_model.into()],
    ))
    .await?;
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO user_settings (user_id, active_model_id)
         SELECT $1, m.id FROM llm_models m JOIN llm_providers p ON p.id = m.provider_id
         WHERE p.name = 'ollama' AND m.name = $2
         ON CONFLICT (user_id) DO UPDATE SET active_model_id = COALESCE(user_settings.active_model_id, EXCLUDED.active_model_id)",
        [owner_id.into(), default_model.into()],
    ))
    .await?;

    let model_bound = |sql: &str| sql.replace("{MODEL}", "(SELECT m.id FROM llm_models m JOIN llm_providers p ON p.id = m.provider_id WHERE p.name = 'ollama' AND m.name = $2)");

    // (legacy table it reads, statement). A table an older install never created is skipped.
    // Order matters: parents before the rows that reference them.
    let steps: [(&str, String); 12] = [
        (
            "settings",
            "UPDATE user_settings u SET name = s.name, timezone = s.timezone, notifications_enabled = s.notifications_enabled
             FROM legacy.settings s WHERE u.user_id = $1"
                .to_string(),
        ),
        (
            "chats",
            model_bound(
                "INSERT INTO chats (id, user_id, name, model_id, is_deleted, created_at, updated_at, summary, summary_up_to_message_id)
                 SELECT id, $1, name, {MODEL}, is_deleted, created_at, updated_at, summary, summary_up_to_message_id
                 FROM legacy.chats",
            ),
        ),
        (
            "chats",
            "INSERT INTO plugin_chats (chat_id, plugin_name, plugin_subname, plugin_chat_id)
             SELECT id, plugin_name, plugin_subname, plugin_chat_id FROM legacy.chats WHERE plugin_name IS NOT NULL"
                .to_string(),
        ),
        (
            "messages",
            "INSERT INTO messages (id, chat_id, role, content, tool_name, thinking, thought_duration_ms, tool_success, tool_denied, created_at)
             SELECT id, chat_id, role, content, tool_name, thinking, thought_duration_ms, tool_success, tool_denied, created_at
             FROM legacy.messages"
                .to_string(),
        ),
        (
            "messages",
            "INSERT INTO message_images (message_id, position, data)
             SELECT m.id, (t.ord - 1)::int, t.val
             FROM legacy.messages m, jsonb_array_elements_text(m.images) WITH ORDINALITY AS t(val, ord)
             WHERE m.images IS NOT NULL AND jsonb_typeof(m.images) = 'array'"
                .to_string(),
        ),
        (
            "files",
            "INSERT INTO files (id, user_id, chat_id, full_path, file_name, read_only)
             SELECT id, $1, chat_id, full_path, file_name, read_only FROM legacy.files"
                .to_string(),
        ),
        (
            "messages",
            "INSERT INTO message_files (message_id, file_id, position)
             SELECT m.id, t.val::bigint, (t.ord - 1)::int
             FROM legacy.messages m, jsonb_array_elements_text(m.file_ids) WITH ORDINALITY AS t(val, ord)
             WHERE m.file_ids IS NOT NULL AND jsonb_typeof(m.file_ids) = 'array'
               AND EXISTS (SELECT 1 FROM files f WHERE f.id = t.val::bigint)
             ON CONFLICT DO NOTHING"
                .to_string(),
        ),
        (
            "tool_calls",
            "INSERT INTO tool_calls (id, message_id, tool_name, arguments, position)
             SELECT id, message_id, tool_name, arguments,
                    (row_number() OVER (PARTITION BY message_id ORDER BY id) - 1)::int
             FROM legacy.tool_calls"
                .to_string(),
        ),
        (
            "tool_permissions",
            "INSERT INTO tool_permissions (id, chat_id, tool_name, scope)
             SELECT id, chat_id, tool_name, scope FROM legacy.tool_permissions"
                .to_string(),
        ),
        (
            // Only a development database from before accounts has this table. The rows come over
            // as they are: a job still marked `running` is one whose process died with the old
            // backend, and `JobStore` marks those `lost` on startup like any other.
            "jobs",
            "INSERT INTO jobs (id, chat_id, command, workdir, log_path, pid, status, exit_code, started_at, finished_at, notified)
             SELECT id, chat_id, command, workdir, log_path, pid, status, exit_code, started_at, finished_at, notified
             FROM legacy.jobs"
                .to_string(),
        ),
        (
            "plugin_settings",
            "INSERT INTO user_plugins (user_id, plugin_name, plugin_subname, settings, enabled)
             SELECT $1, plugin_name, plugin_subname, settings, enabled FROM legacy.plugin_settings"
                .to_string(),
        ),
        (
            "chats",
            // Explicit ids were inserted above, so the sequences still sit at 1: move each past
            // the highest copied id. One statement to keep the `steps` array uniform.
            "SELECT setval(pg_get_serial_sequence('chats', 'id'), COALESCE((SELECT MAX(id) FROM chats), 0) + 1, false),
                    setval(pg_get_serial_sequence('messages', 'id'), COALESCE((SELECT MAX(id) FROM messages), 0) + 1, false),
                    setval(pg_get_serial_sequence('files', 'id'), COALESCE((SELECT MAX(id) FROM files), 0) + 1, false),
                    setval(pg_get_serial_sequence('tool_calls', 'id'), COALESCE((SELECT MAX(id) FROM tool_calls), 0) + 1, false),
                    setval(pg_get_serial_sequence('tool_permissions', 'id'), COALESCE((SELECT MAX(id) FROM tool_permissions), 0) + 1, false),
                    setval(pg_get_serial_sequence('jobs', 'id'), COALESCE((SELECT MAX(id) FROM jobs), 0) + 1, false)
             WHERE $1 IS NOT NULL"
                .to_string(),
        ),
    ];

    for (legacy_table, sql) in steps {
        if !table_exists(&txn, &format!("{LEGACY_SCHEMA}.{legacy_table}")).await? {
            continue;
        }
        // Statements reference $1 (the owner) and, for chats, $2 (the model); Postgres needs
        // every placeholder present in the statement to be bound, and none extra.
        let values: Vec<sea_orm::Value> = if sql.contains("$2") {
            vec![owner[0].clone(), default_model.into()]
        } else {
            vec![owner[0].clone()]
        };
        txn.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres, sql, values)).await?;
    }

    // The compaction key facts exist only on chats from a development database (the released
    // single-user schema has no such column), so the copy depends on the column being there.
    if column_exists(&txn, LEGACY_SCHEMA, "chats", "key_facts").await? {
        txn.execute_unprepared(
            "UPDATE chats c SET key_facts = l.key_facts FROM legacy.chats l WHERE c.id = l.id AND l.key_facts IS NOT NULL",
        )
        .await?;
    }

    // Kept as a backup; a timestamp suffix so a second adoption (a re-created legacy schema)
    // can't collide with the first one's leftovers.
    txn.execute_unprepared(&format!(
        "ALTER SCHEMA {LEGACY_SCHEMA} RENAME TO legacy_adopted_{}",
        chrono::Utc::now().timestamp()
    ))
    .await?;

    txn.commit().await?;
    Ok(true)
}
