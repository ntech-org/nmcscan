use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Add scan_epoch column to asn_ranges for deterministic shuffle seed.
        // This replaces the Redis epoch counter (scan_epochs hash).
        db.execute_unprepared(
            "ALTER TABLE asn_ranges ADD COLUMN IF NOT EXISTS scan_epoch BIGINT DEFAULT 0"
        ).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("ALTER TABLE asn_ranges DROP COLUMN IF EXISTS scan_epoch").await?;
        Ok(())
    }
}
