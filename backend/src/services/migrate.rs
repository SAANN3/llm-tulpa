use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, Statement, TransactionTrait};

/// The schema version this build creates and expects. Recorded in `schema_meta`.
///
/// Moving to a new version is a data-preserving step, never a wipe: add a
/// `if current < N { ... }` block to `run_migrations` that alters the existing tables in place
/// (inside the same transaction) and bump this constant. A database *newer* than this build is
/// refused rather than "fixed", so an older binary can't damage data a newer one wrote.
const SCHEMA_VERSION: i32 = 15;

/// Bumped when a release changes what the setup wizard configures. An install whose
/// `schema_meta.setup_revision` is behind is offered the wizard again, with its current answers kept.
pub const SETUP_REVISION: i32 = 1;

/// The setup revision this database's owner last completed.
pub async fn setup_revision(db: &DatabaseConnection) -> Result<i32, DbErr> {
    let row = db
        .query_one_raw(Statement::from_string(DbBackend::Postgres, "SELECT setup_revision FROM schema_meta WHERE id = TRUE"))
        .await?;
    Ok(row.map(|r| r.try_get::<i32>("", "setup_revision")).transpose()?.unwrap_or(0))
}

/// Records that the owner has been through the current setup.
pub async fn complete_setup(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(&format!("UPDATE schema_meta SET setup_revision = {SETUP_REVISION} WHERE id = TRUE")).await?;
    Ok(())
}

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

/// What version 4 added on top of version 3: ground-truth prompt tokens on chats and token metrics on messages.
const V4_ADDITIONS: &str = "
    -- Ground-truth prompt token count from Ollama's last evaluated turn; NULL after compaction fold
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS last_prompt_tokens BIGINT;

    -- Per-message token metrics reported by Ollama
    ALTER TABLE messages ADD COLUMN IF NOT EXISTS prompt_tokens BIGINT;
    ALTER TABLE messages ADD COLUMN IF NOT EXISTS eval_tokens BIGINT;
";

/// What version 5 added on top of version 4: a per-user system prompt.
const V5_ADDITIONS: &str = "
    -- The user's own system prompt, replacing the built-in default for their chats; NULL means
    -- the hardcoded default applies.
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS system_prompt TEXT;
";

/// What version 6 added on top of version 5: chat folders.
const V6_ADDITIONS: &str = "
    -- A user's own grouping of chats. No soft delete: a folder holds no independent history
    -- worth recovering, unlike chats/messages.
    CREATE TABLE IF NOT EXISTS folders (
        id BIGSERIAL PRIMARY KEY,
        user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    CREATE INDEX IF NOT EXISTS idx_folders_user ON folders (user_id);

    -- A chat's folder is optional and survives its folder going away (SET NULL, not CASCADE) —
    -- deleting a folder ungroups its chats rather than deleting them.
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS folder_id BIGINT REFERENCES folders (id) ON DELETE SET NULL;
    CREATE INDEX IF NOT EXISTS idx_chats_folder ON chats (folder_id);
";

/// What version 7 added on top of version 6: sub-agent chats and the auto-confirm setting.
const V7_ADDITIONS: &str = "
    -- A sub-agent's chat points at the chat that delegated to it; NULL for every ordinary chat.
    -- CASCADE: a sub-chat has no meaning without its parent, and chats are only ever soft-deleted
    -- anyway, so this fires only when a whole user (and with them their chats) goes away.
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS parent_chat_id BIGINT REFERENCES chats (id) ON DELETE CASCADE;
    CREATE INDEX IF NOT EXISTS idx_chats_parent ON chats (parent_chat_id);

    -- Whether tool-permission prompts are approved automatically, for the user's chats and for
    -- the sub-agents those chats start (nobody is watching a sub-agent to answer a prompt).
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS auto_confirm BOOLEAN NOT NULL DEFAULT false;
";

/// What version 8 added on top of version 7: sub-agents as background jobs.
const V8_ADDITIONS: &str = "
    -- A job is either a shell command or a sub-agent. A sub-agent's job keeps the chat it runs in;
    -- its `command` is the prompt it was given, and its log file holds the result it handed back.
    ALTER TABLE jobs ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'process';
    ALTER TABLE jobs ADD COLUMN IF NOT EXISTS agent_chat_id BIGINT REFERENCES chats (id) ON DELETE CASCADE;
    ALTER TABLE jobs ADD CONSTRAINT jobs_kind_valid
        CHECK (kind IN ('process', 'agent') AND ((kind = 'agent') = (agent_chat_id IS NOT NULL)));
";

/// What version 9 added on top of version 8: the timings Ollama reports for each reply.
const V9_ADDITIONS: &str = "
    -- Per-reply timings from the model backend, in milliseconds: generating the reply, evaluating
    -- its prompt, and loading the model, plus how many prompt tokens were actually evaluated (the
    -- rest came from the server's cache, and cost nothing). NULL on replies from before they were
    -- recorded (they were only logged), and on any call whose backend doesn't report them. With
    -- the token counts they give real tokens/second, which `thought_duration_ms` (the whole call,
    -- cache hits and model loading included) can't.
    ALTER TABLE messages ADD COLUMN IF NOT EXISTS eval_duration_ms BIGINT;
    ALTER TABLE messages ADD COLUMN IF NOT EXISTS prompt_eval_duration_ms BIGINT;
    ALTER TABLE messages ADD COLUMN IF NOT EXISTS load_duration_ms BIGINT;
    ALTER TABLE messages ADD COLUMN IF NOT EXISTS prompt_processed_tokens BIGINT;
";

/// What version 10 added on top of version 9: the managed llama.cpp provider, how a model is
/// started (launch profiles, global), how it samples (presets, per user), and which launch profile
/// a chat runs on.
const V10_ADDITIONS: &str = "
    -- The llama.cpp server the backend runs itself, next to Ollama.
    INSERT INTO llm_providers (name) VALUES ('llama-cpp') ON CONFLICT (name) DO NOTHING;

    -- A name to show instead of the model's identity (a file name or an Ollama tag).
    ALTER TABLE llm_models ADD COLUMN IF NOT EXISTS display_name TEXT;

    -- How a model is started. Global, because it is the hardware's business: the owner writes them,
    -- everyone reads and chooses among them. A model can have several (a long-context one, a
    -- vision one). NULL `context_length` sizes the context to free memory (llama-server's --fit).
    CREATE TABLE IF NOT EXISTS launch_profiles (
        id BIGSERIAL PRIMARY KEY,
        model_id BIGINT NOT NULL REFERENCES llm_models (id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        mmproj_file TEXT,
        mmproj_gpu BOOLEAN NOT NULL DEFAULT TRUE,
        context_length INT,
        cache_type_k TEXT NOT NULL DEFAULT 'q8_0',
        cache_type_v TEXT NOT NULL DEFAULT 'q8_0',
        flash_attn BOOLEAN NOT NULL DEFAULT TRUE,
        gpu_layers INT NOT NULL DEFAULT 99,
        mtp BOOLEAN NOT NULL DEFAULT TRUE,
        spec_draft_n_max INT NOT NULL DEFAULT 3,
        ngram_match INT NOT NULL DEFAULT 24,
        ngram_min INT NOT NULL DEFAULT 8,
        ngram_max INT NOT NULL DEFAULT 32,
        extra_args TEXT NOT NULL DEFAULT '',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        CONSTRAINT launch_profiles_name_unique UNIQUE (model_id, name),
        CONSTRAINT launch_profiles_context_valid CHECK (context_length IS NULL OR context_length >= 512),
        CONSTRAINT launch_profiles_cache_k_valid CHECK (cache_type_k IN ('f32', 'f16', 'bf16', 'q8_0', 'q4_0', 'q4_1', 'iq4_nl', 'q5_0', 'q5_1')),
        CONSTRAINT launch_profiles_cache_v_valid CHECK (cache_type_v IN ('f32', 'f16', 'bf16', 'q8_0', 'q4_0', 'q4_1', 'iq4_nl', 'q5_0', 'q5_1'))
    );

    -- The profile a chat runs on, which implies its model. NULL for a chat on a model that has no
    -- launch profiles (an Ollama model, whose server decides how it runs).
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS launch_profile_id BIGINT REFERENCES launch_profiles (id) ON DELETE SET NULL;

    -- A user's named sampling settings. NULL `model_id` means any model; NULL values are not sent,
    -- so the server's default applies. Export and import go through JSON, not this table.
    CREATE TABLE IF NOT EXISTS sampling_presets (
        id BIGSERIAL PRIMARY KEY,
        user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        model_id BIGINT REFERENCES llm_models (id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        temperature REAL,
        top_p REAL,
        top_k INT,
        min_p REAL,
        repeat_penalty REAL,
        presence_penalty REAL,
        seed BIGINT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    CREATE UNIQUE INDEX IF NOT EXISTS sampling_presets_name_unique ON sampling_presets (user_id, COALESCE(model_id, 0), name);

    -- The preset a user has chosen for a model; it goes with the preset when that is deleted.
    CREATE TABLE IF NOT EXISTS user_preset_choice (
        user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        model_id BIGINT NOT NULL REFERENCES llm_models (id) ON DELETE CASCADE,
        preset_id BIGINT NOT NULL REFERENCES sampling_presets (id) ON DELETE CASCADE,
        PRIMARY KEY (user_id, model_id)
    );

    -- A user's Hugging Face token, for downloading gated models.
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS hf_token TEXT;

    -- Which revision of the model setup this install has been through. The owner is asked to go
    -- through it again when the build's revision is newer; every install starts at 0.
    ALTER TABLE schema_meta ADD COLUMN IF NOT EXISTS setup_revision INT NOT NULL DEFAULT 0;
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

/// What version 11 added on top of version 10: the launch profile a user picked as their default, which
/// new chats start on (their default model's first profile when it is empty).
const V11_ADDITIONS: &str = "
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS active_profile_id BIGINT REFERENCES launch_profiles (id) ON DELETE SET NULL;
";

/// What version 12 added on top of version 11: the name of the model a sampling preset was made for
/// when that model was removed. Such a preset stays, for any model, and says where it came from.
const V12_ADDITIONS: &str = "
    ALTER TABLE sampling_presets ADD COLUMN IF NOT EXISTS removed_model TEXT;
";

/// What version 13 added on top of version 12: the agent's own working notes for a chat (kept across
/// compaction folds; `notes` is what the prompt carries, `notes_pending` what `chat.write_notes` saved since the
/// last compaction, which joins the prompt at the next one), the id up to which a chat's old tool results are
/// shown as one-line stubs instead of in full, the same for old thinking traces (shortened to their tail), and
/// the user's choice to have old thinking shortened at all.
const V13_ADDITIONS: &str = "
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS notes TEXT;
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS notes_pending TEXT;
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS cleared_up_to_message_id BIGINT;
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS thinking_trimmed_up_to_message_id BIGINT;
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS trim_old_thinking BOOLEAN NOT NULL DEFAULT FALSE;
";

/// What version 14 added on top of version 13: how many model calls a turn may make before it is
/// stopped at the user's request, `NULL` for no limit.
const V14_ADDITIONS: &str = "
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS max_turn_steps INT CHECK (max_turn_steps IS NULL OR max_turn_steps > 0);
";

/// What version 15 added on top of version 14: whether a chat sends the model its tools (a small model, or a
/// window the tool definitions would fill, runs without them), and the user's default for new chats.
const V15_ADDITIONS: &str = "
    ALTER TABLE chats ADD COLUMN IF NOT EXISTS tools_enabled BOOLEAN NOT NULL DEFAULT TRUE;
    ALTER TABLE user_settings ADD COLUMN IF NOT EXISTS use_tools BOOLEAN NOT NULL DEFAULT TRUE;
";

/// Ensures the schema exists at `SCHEMA_VERSION`. Idempotent: at the current version it does
/// nothing. Run once at bootstrap against a connection already pointed at the target database.
///
/// - **Fresh database** → creates the schema.
/// - **Single-user database** (the release before accounts existed: a `chats` table with no
///   `user_id`) → its tables are *moved*, not dropped, into a `legacy` schema and the new schema
///   is created next to them. There's no user to own that data yet, so it stays there until the
///   owner account is created, at which point `adopt_legacy_data` copies it in.
/// - **Version 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13 or 14** → upgraded in place to the current version, nothing dropped.
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
        Some(2) => return upgrade_v2_to_v15(db).await,
        Some(3) => return upgrade_v3_to_v15(db).await,
        Some(4) => return upgrade_v4_to_v15(db).await,
        Some(5) => return upgrade_v5_to_v15(db).await,
        Some(6) => return upgrade_v6_to_v15(db).await,
        Some(7) => return upgrade_v7_to_v15(db).await,
        Some(8) => return upgrade_v8_to_v15(db).await,
        Some(9) => return upgrade_v9_to_v15(db).await,
        Some(10) => return upgrade_v10_to_v15(db).await,
        Some(11) => return upgrade_v11_to_v15(db).await,
        Some(12) => return upgrade_v12_to_v15(db).await,
        Some(13) => return upgrade_v13_to_v15(db).await,
        Some(14) => return upgrade_v14_to_v15(db).await,
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

/// Version 14 → 15, in place: adds `chats.tools_enabled` and `user_settings.use_tools`. One transaction.
async fn upgrade_v14_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 13 → 15, in place: adds `user_settings.max_turn_steps`. One transaction.
async fn upgrade_v13_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 12 → 15, in place: adds `chats.notes`, `chats.notes_pending`, `chats.cleared_up_to_message_id`, `chats.thinking_trimmed_up_to_message_id` and `user_settings.trim_old_thinking`, and what version 14 added. One transaction.
async fn upgrade_v12_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 2 → 15, in place: adds `chats.key_facts`, `jobs` table, version 4 token columns,
/// `user_settings.system_prompt`, the `folders` table, `chats.parent_chat_id`,
/// `user_settings.auto_confirm`, `jobs.kind`/`jobs.agent_chat_id`, the per-reply timing columns, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added. One transaction.
async fn upgrade_v2_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V3_ADDITIONS).await?;
    txn.execute_unprepared(V4_ADDITIONS).await?;
    txn.execute_unprepared(V5_ADDITIONS).await?;
    txn.execute_unprepared(V6_ADDITIONS).await?;
    txn.execute_unprepared(V7_ADDITIONS).await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 3 → 15, in place: adds `chats.last_prompt_tokens`, `messages.prompt_tokens`,
/// `messages.eval_tokens`, `user_settings.system_prompt`, the `folders` table,
/// `chats.parent_chat_id`, `user_settings.auto_confirm`, `jobs.kind`/`jobs.agent_chat_id`, the per-reply timing columns, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v3_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V4_ADDITIONS).await?;
    txn.execute_unprepared(V5_ADDITIONS).await?;
    txn.execute_unprepared(V6_ADDITIONS).await?;
    txn.execute_unprepared(V7_ADDITIONS).await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 4 → 15, in place: adds the per-user `system_prompt` column, the `folders` table,
/// `chats.parent_chat_id`, `user_settings.auto_confirm`, `jobs.kind`/`jobs.agent_chat_id`, the per-reply timing columns, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v4_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V5_ADDITIONS).await?;
    txn.execute_unprepared(V6_ADDITIONS).await?;
    txn.execute_unprepared(V7_ADDITIONS).await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 5 → 15, in place: adds chat folders, `chats.parent_chat_id`, `user_settings.auto_confirm`,
/// `jobs.kind`/`jobs.agent_chat_id`, the per-reply timing columns, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v5_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V6_ADDITIONS).await?;
    txn.execute_unprepared(V7_ADDITIONS).await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 6 → 15, in place: adds `chats.parent_chat_id`, `user_settings.auto_confirm`, and
/// `jobs.kind`/`jobs.agent_chat_id`, the per-reply timing columns, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v6_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V7_ADDITIONS).await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 7 → 15, in place: adds `jobs.kind`/`jobs.agent_chat_id` the per-reply timing columns, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v7_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 11 → 15, in place: adds `sampling_presets.removed_model` what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v11_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 10 → 15, in place: adds `user_settings.active_profile_id` and `sampling_presets.removed_model`, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v10_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 9 → 15, in place: adds the managed llama.cpp provider, launch profiles, sampling presets and
/// their choices, `chats.launch_profile_id`, `llm_models.display_name`, `user_settings.hf_token` and
/// `schema_meta.setup_revision`.
async fn upgrade_v9_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
    txn.execute_unprepared(&format!("UPDATE schema_meta SET version = {SCHEMA_VERSION} WHERE id = TRUE"))
        .await?;
    txn.commit().await
}

/// Version 8 → 15, in place: adds the per-reply timing columns on `messages`,, what version 10 added, what version 11 added, what version 12 added, what version 13 added, what version 14 added and what version 15 added.
async fn upgrade_v8_to_v15(db: &DatabaseConnection) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
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
    txn.execute_unprepared(V4_ADDITIONS).await?;
    txn.execute_unprepared(V5_ADDITIONS).await?;
    txn.execute_unprepared(V6_ADDITIONS).await?;
    txn.execute_unprepared(V7_ADDITIONS).await?;
    txn.execute_unprepared(V8_ADDITIONS).await?;
    txn.execute_unprepared(V9_ADDITIONS).await?;
    txn.execute_unprepared(V10_ADDITIONS).await?;
    txn.execute_unprepared(V11_ADDITIONS).await?;
    txn.execute_unprepared(V12_ADDITIONS).await?;
    txn.execute_unprepared(V13_ADDITIONS).await?;
    txn.execute_unprepared(V14_ADDITIONS).await?;
    txn.execute_unprepared(V15_ADDITIONS).await?;
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
