use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Add created_at column — true "first seen" timestamp, set only on INSERT.
        // For existing rows, default to last_seen (best approximation of first seen).
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP",
        )
        .await?;

        // Backfill: for existing servers, use last_seen as created_at if created_at is null
        // (this happens because the DEFAULT only applies to new INSERTs after the column was added,
        // but ALTER TABLE with DEFAULT should backfill. Just to be safe, we also do this:)
        db.execute_unprepared(
            "UPDATE servers SET created_at = last_seen WHERE created_at IS NULL AND last_seen IS NOT NULL",
        )
        .await?;

        // Also set created_at = CURRENT_TIMESTAMP for any rows that never had last_seen
        db.execute_unprepared(
            "UPDATE servers SET created_at = CURRENT_TIMESTAMP WHERE created_at IS NULL",
        )
        .await?;

        // Trigger to prevent updating created_at after insert — it should be immutable
        db.execute_unprepared(r#"
            CREATE OR REPLACE FUNCTION preserve_created_at() RETURNS trigger AS $$
            BEGIN
              NEW.created_at := OLD.created_at;
              RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#).await?;

        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_preserve_created_at ON servers")
            .await?;

        db.execute_unprepared(r#"
            CREATE TRIGGER trg_preserve_created_at
              BEFORE UPDATE ON servers
              FOR EACH ROW
              EXECUTE FUNCTION preserve_created_at()
        "#).await?;

        // Index for sorting and filtering by created_at
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_created_at ON servers(created_at DESC)",
        )
        .await?;

        // Composite index for the common "online servers ordered by first seen" query
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_status_created ON servers(status, created_at DESC)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_preserve_created_at ON servers")
            .await?;
        db.execute_unprepared("DROP FUNCTION IF EXISTS preserve_created_at()")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_created_at")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_status_created")
            .await?;
        db.execute_unprepared("ALTER TABLE servers DROP COLUMN IF EXISTS created_at")
            .await?;

        Ok(())
    }
}
