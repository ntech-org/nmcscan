use sea_orm_migration::prelude::*;

/// Migration to add optimized indexes only.
///
/// IP and port column types are kept as TEXT/INTEGER for maximum
/// compatibility with SeaORM's type system. The indexes added here
/// improve search performance significantly.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        println!("Starting database index optimization migration...");

        // ── Step 1: Drop materialized views that depend on servers table ─────
        println!("Dropping materialized views for recreation...");
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS asn_stats")
            .await?;
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS global_stats")
            .await?;

        // ── Step 2: Drop indexes we'll recreate ──────────────────────────────
        println!("Dropping indexes to be recreated...");
        db.execute_unprepared("DROP INDEX IF EXISTS trgm_idx_servers_ip")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_server_history_ip_port")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_asn_status")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_status_players_ip")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_country_players_ip")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_asn_players_ip")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_type_players_ip")
            .await?;

        // ── Step 3: Recreate indexes ─────────────────────────────────────────
        println!("Recreating indexes...");

        // History index
        db.execute_unprepared(
            "CREATE INDEX idx_server_history_ip_port ON server_history(ip, port)",
        )
        .await?;

        // IP trigram index
        db.execute_unprepared(
            "CREATE INDEX trgm_idx_servers_ip ON servers USING GIN (ip gin_trgm_ops)",
        )
        .await?;

        // Composite indexes on servers
        db.execute_unprepared(
            "CREATE INDEX idx_servers_asn_status ON servers(asn, status) INCLUDE (ip)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_status_players_ip ON servers(status, players_online DESC, ip ASC)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_country_players_ip ON servers(country, players_online DESC, ip ASC)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_asn_players_ip ON servers(asn, players_online DESC, ip ASC)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_type_players_ip ON servers(server_type, players_online DESC, ip ASC)",
        )
        .await?;

        // ── Step 4: Add new indexes ──────────────────────────────────────────
        println!("Adding new optimized indexes...");

        // Player name trigram search
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS trgm_idx_server_players_player_name \
             ON server_players USING GIN (player_name gin_trgm_ops)",
        )
        .await?;

        // ASN tags full-text search
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_asns_tags_gin \
             ON asns USING GIN (to_tsvector('simple', COALESCE(tags, '')))",
        )
        .await?;

        // Partial index for flag filtering
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_flags_status \
             ON servers (status) WHERE flags IS NOT NULL AND flags != ''",
        )
        .await?;

        // ── Step 5: Recreate materialized views ──────────────────────────────
        println!("Recreating materialized views...");

        db.execute_unprepared(
            "CREATE MATERIALIZED VIEW asn_stats AS
            SELECT a.asn, a.org, a.category, a.country, a.tags, a.last_updated,
                   COUNT(s.ip)::bigint as server_count
            FROM asns a
            LEFT JOIN servers s ON s.asn = a.asn AND s.status != 'ignored'
            GROUP BY a.asn, a.org, a.category, a.country, a.tags, a.last_updated",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_asn_stats_asn ON asn_stats(asn)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE MATERIALIZED VIEW global_stats AS
            SELECT
              (SELECT COUNT(*)::bigint FROM servers WHERE status != 'ignored') as server_count,
              (SELECT COUNT(*)::bigint FROM servers WHERE status = 'online') as online_count,
              (SELECT COALESCE(SUM(players_online), 0)::bigint FROM servers WHERE status = 'online') as total_players,
              1 as id",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_global_stats_id ON global_stats(id)",
        )
        .await?;

        println!("Database optimization complete!");
        println!("Indexes added/recreated:");
        println!("  - trgm_idx_servers_ip: IP trigram search");
        println!("  - idx_server_history_ip_port: History lookup");
        println!("  - idx_servers_asn_status: ASN + status composite");
        println!("  - idx_servers_status_players_ip: Status + players sort");
        println!("  - idx_servers_country_players_ip: Country + players sort");
        println!("  - idx_servers_asn_players_ip: ASN + players sort");
        println!("  - idx_servers_type_players_ip: Type + players sort");
        println!("  - trgm_idx_server_players_player_name: Player name search");
        println!("  - idx_asns_tags_gin: ASN tag full-text search");
        println!("  - idx_servers_flags_status: Partial flag index");

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        println!("Rolling back database optimization...");

        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS asn_stats")
            .await?;
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS global_stats")
            .await?;

        // Drop new indexes
        db.execute_unprepared("DROP INDEX IF EXISTS trgm_idx_server_players_player_name")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_asns_tags_gin")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_flags_status")
            .await?;

        // Recreate original indexes
        db.execute_unprepared(
            "CREATE INDEX idx_server_history_ip_port ON server_history(ip, port)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX trgm_idx_servers_ip ON servers USING GIN (ip gin_trgm_ops)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_asn_status ON servers(asn, status) INCLUDE (ip)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_status_players_ip ON servers(status, players_online DESC, ip ASC)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_country_players_ip ON servers(country, players_online DESC, ip ASC)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_asn_players_ip ON servers(asn, players_online DESC, ip ASC)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_servers_type_players_ip ON servers(server_type, players_online DESC, ip ASC)",
        )
        .await?;

        // Recreate materialized views
        db.execute_unprepared(
            "CREATE MATERIALIZED VIEW asn_stats AS
            SELECT a.asn, a.org, a.category, a.country, a.tags, a.last_updated,
                   COUNT(s.ip)::bigint as server_count
            FROM asns a
            LEFT JOIN servers s ON s.asn = a.asn AND s.status != 'ignored'
            GROUP BY a.asn, a.org, a.category, a.country, a.tags, a.last_updated",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_asn_stats_asn ON asn_stats(asn)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE MATERIALIZED VIEW global_stats AS
            SELECT
              (SELECT COUNT(*)::bigint FROM servers WHERE status != 'ignored') as server_count,
              (SELECT COUNT(*)::bigint FROM servers WHERE status = 'online') as online_count,
              (SELECT COALESCE(SUM(players_online), 0)::bigint FROM servers WHERE status = 'online') as total_players,
              1 as id",
        )
        .await?;
        db.execute_unprepared(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_global_stats_id ON global_stats(id)",
        )
        .await?;

        println!("Database optimization rollback complete.");
        Ok(())
    }
}
