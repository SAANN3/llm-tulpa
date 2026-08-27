use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// Ensures the schema exists — idempotent `CREATE TABLE` run on every boot, same
/// deliberate no-migration-history approach as `chat_store::migrate`. The unique
/// constraint on `(plugin_name, plugin_subname)` is what makes `PluginSettingsStore`'s
/// upsert well-defined — at most one persisted row per plugin instance.
pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "
        CREATE TABLE IF NOT EXISTS plugin_settings (
            id BIGSERIAL PRIMARY KEY,
            plugin_name TEXT NOT NULL,
            plugin_subname TEXT NOT NULL,
            settings JSONB NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT false,
            CONSTRAINT plugin_settings_key_unique UNIQUE (plugin_name, plugin_subname)
        );
        ",
    )
    .await?;

    Ok(())
}
