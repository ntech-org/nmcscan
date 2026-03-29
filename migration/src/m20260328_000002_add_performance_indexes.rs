use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Optimized composite indexes for common filter + sort combinations
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_status_players_ip ON servers(status, players_online DESC, ip ASC)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_country_players_ip ON servers(country, players_online DESC, ip ASC)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_asn_players_ip ON servers(asn, players_online DESC, ip ASC)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_type_players_ip ON servers(server_type, players_online DESC, ip ASC)").await?;
        
        // Index for brand search (substring)
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS trgm_idx_servers_brand ON servers USING GIN (brand gin_trgm_ops)").await?;
        
        // Index for max players (capacity) filtering
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_players_max ON servers(players_max)").await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_status_players_ip").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_country_players_ip").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_asn_players_ip").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_type_players_ip").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS trgm_idx_servers_brand").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_players_max").await?;

        Ok(())
    }
}
