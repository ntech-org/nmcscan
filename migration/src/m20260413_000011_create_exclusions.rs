use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Exclusions table — replaces file-based exclude.conf for runtime management.
        // The static exclude.conf is still loaded at startup for initial seeding,
        // but all runtime CRUD goes through this table.
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS exclusions (
                id SERIAL PRIMARY KEY,
                network TEXT NOT NULL UNIQUE,
                comment TEXT DEFAULT '',
                created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
                source TEXT NOT NULL DEFAULT 'manual'
                    CHECK (source IN ('manual', 'config_file', 'honeypot'))
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_exclusions_network ON exclusions(network)",
        )
        .await?;

        // Seed existing exclude.conf entries into the DB on migration.
        // This is a one-time operation — the file will still be parsed at startup
        // to catch any entries not yet in the DB, and they'll be upserted.
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS exclusions").await?;
        Ok(())
    }
}
