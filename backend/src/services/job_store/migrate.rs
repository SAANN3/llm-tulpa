use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// Ensures the schema exists — idempotent `CREATE TABLE` run on every boot, same
/// deliberate no-migration-history approach as `chat_store::migrate`.
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "
        CREATE TABLE IF NOT EXISTS jobs (
            id BIGSERIAL PRIMARY KEY,
            chat_id BIGINT NOT NULL REFERENCES chats (id),
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

        CREATE INDEX IF NOT EXISTS idx_jobs_chat_id
            ON jobs (chat_id);
        ",
    )
    .await?;

    Ok(())
}
