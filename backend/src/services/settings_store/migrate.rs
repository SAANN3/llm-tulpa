use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// Ensures the schema exists — idempotent `CREATE TABLE` run on every boot, same
/// deliberate no-migration-history approach as `chat_store::migrate`. The `CHECK (id =
/// 1)` constraint is what actually enforces "at most one row" — simpler than a unique
/// index on a constant, and it reads as intent at the schema level.
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "
        CREATE TABLE IF NOT EXISTS settings (
            id SMALLINT PRIMARY KEY DEFAULT 1,
            name TEXT NOT NULL,
            timezone SMALLINT NOT NULL,
            CONSTRAINT settings_singleton CHECK (id = 1)
        );

        -- `CREATE TABLE IF NOT EXISTS` above is a no-op once the table already exists,
        -- so columns added after the table's first deploy need their own idempotent
        -- statement here rather than editing the `CREATE TABLE` block itself.
        ALTER TABLE settings ADD COLUMN IF NOT EXISTS notifications_enabled BOOLEAN NOT NULL DEFAULT false;
        ",
    )
    .await?;

    Ok(())
}
