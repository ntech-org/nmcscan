use sea_orm_migration::prelude::*;

/// Migration to convert column types and add optimized indexes.
///
/// - IP columns: TEXT → INET (via SeaORM's IpNetwork type)
/// - Port columns: INTEGER → SMALLINT
/// - Drops/recreates FK constraints, materialized views, and indexes that depend on these columns
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        println!("Starting database optimization migration...");

        // ── Step 0: Drop all dependencies on ip/port columns ─────────────────

        // Materialized views that reference ip/port
        println!("Dropping materialized views...");
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS asn_stats")
            .await?;
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS global_stats")
            .await?;

        // Foreign keys on (ip, port) — must drop before altering column types
        println!("Dropping foreign key constraints...");
        db.execute_unprepared(
            "ALTER TABLE server_players DROP CONSTRAINT IF EXISTS server_players_ip_port_fkey",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history DROP CONSTRAINT IF EXISTS server_history_ip_port_fkey",
        )
        .await?;

        // Indexes that reference ip or port columns — must drop before type change
        println!("Dropping dependent indexes...");
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

        // ── Step 1: Convert IP columns from TEXT to INET ─────────────────────
        println!("Converting IP columns from TEXT to INET...");

        db.execute_unprepared("ALTER TABLE servers ALTER COLUMN ip TYPE INET USING ip::INET")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE server_players ALTER COLUMN ip TYPE INET USING ip::INET",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history ALTER COLUMN ip TYPE INET USING ip::INET",
        )
        .await?;

        println!("IP columns converted to INET.");

        // ── Step 2: Convert port columns from INTEGER to SMALLINT ────────────
        println!("Converting port columns from INTEGER to SMALLINT...");

        db.execute_unprepared(
            "ALTER TABLE servers ALTER COLUMN port TYPE SMALLINT USING port::SMALLINT",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_players ALTER COLUMN port TYPE SMALLINT USING port::SMALLINT",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history ALTER COLUMN port TYPE SMALLINT USING port::SMALLINT",
        )
        .await?;

        println!("Port columns converted to SMALLINT.");

        // ── Step 3: Recreate foreign keys ────────────────────────────────────
        println!("Recreating foreign key constraints...");

        db.execute_unprepared(
            "ALTER TABLE server_players ADD CONSTRAINT server_players_ip_port_fkey \
             FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history ADD CONSTRAINT server_history_ip_port_fkey \
             FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE",
        )
        .await?;

        // ── Step 4: Recreate materialized views ──────────────────────────────
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

        // ── Step 5: Recreate and add indexes ─────────────────────────────────
        println!("Recreating and adding indexes...");

        // History index on (ip, port)
        db.execute_unprepared(
            "CREATE INDEX idx_server_history_ip_port ON server_history(ip, port)",
        )
        .await?;

        // IP trigram index as functional index (INET → text cast for GIN)
        db.execute_unprepared(
            "CREATE INDEX trgm_idx_servers_ip ON servers USING GIN (((ip)::text) gin_trgm_ops)",
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

        // New optimized indexes
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS trgm_idx_server_players_player_name \
             ON server_players USING GIN (player_name gin_trgm_ops)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_asns_tags_gin \
             ON asns USING GIN (to_tsvector('simple', COALESCE(tags, '')))",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_flags_status \
             ON servers (status) WHERE flags IS NOT NULL AND flags != ''",
        )
        .await?;

        println!("Database optimization complete!");
        println!("Changes applied:");
        println!("  - IP columns: TEXT → INET (7-19 bytes, native validation)");
        println!("  - Port columns: INTEGER → SMALLINT (2 bytes saved each)");
        println!("  - All FK constraints, views, and indexes recreated");
        println!("  - New indexes: player name trigram, ASN tags GIN, flags partial");

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        println!("Rolling back database optimization...");

        // Drop everything that depends on ip/port
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS asn_stats")
            .await?;
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS global_stats")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE server_players DROP CONSTRAINT IF EXISTS server_players_ip_port_fkey",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history DROP CONSTRAINT IF EXISTS server_history_ip_port_fkey",
        )
        .await?;
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
        db.execute_unprepared("DROP INDEX IF EXISTS trgm_idx_server_players_player_name")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_asns_tags_gin")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_flags_status")
            .await?;

        // Revert types
        db.execute_unprepared(
            "ALTER TABLE server_history ALTER COLUMN port TYPE INTEGER USING port::INTEGER",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_players ALTER COLUMN port TYPE INTEGER USING port::INTEGER",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE servers ALTER COLUMN port TYPE INTEGER USING port::INTEGER",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history ALTER COLUMN ip TYPE TEXT USING ip::TEXT",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_players ALTER COLUMN ip TYPE TEXT USING ip::TEXT",
        )
        .await?;
        db.execute_unprepared("ALTER TABLE servers ALTER COLUMN ip TYPE TEXT USING ip::TEXT")
            .await?;

        // Recreate FKs
        db.execute_unprepared(
            "ALTER TABLE server_players ADD CONSTRAINT server_players_ip_port_fkey \
             FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE server_history ADD CONSTRAINT server_history_ip_port_fkey \
             FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE",
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

        println!("Database optimization rollback complete.");

        Ok(())
    }
}
