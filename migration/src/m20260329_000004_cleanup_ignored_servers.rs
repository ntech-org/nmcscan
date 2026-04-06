use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Delete all ignored servers (offline scans tracked in Redis bitset now).
        // CASCADE deletes their server_history and server_players entries too.
        db.execute_unprepared("DELETE FROM servers WHERE status = 'ignored'")
            .await?;

        // Add index on server_history timestamp for efficient capping queries
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_server_history_ts ON server_history(ip, port, timestamp ASC)"
        ).await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Cannot restore deleted data
        Ok(())
    }
}
