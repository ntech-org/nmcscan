use sea_orm_migration::prelude::*;

#[allow(dead_code)]
#[derive(Iden)]
enum Servers {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum ServerPlayers {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum ServerHistory {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum Asns {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum AsnRanges {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum DailyStats {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum Users {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum Accounts {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum Sessions {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum VerificationToken {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum AsnStats {
    Table,
}

#[allow(dead_code)]
#[derive(Iden)]
enum GlobalStats {
    Table,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Trigram extension
        db.execute_unprepared("CREATE EXTENSION IF NOT EXISTS pg_trgm").await?;

        // Tables
        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS servers (
                ip TEXT,
                port INTEGER DEFAULT 25565,
                server_type TEXT DEFAULT 'java',
                status TEXT DEFAULT 'unknown',
                players_online INTEGER DEFAULT 0,
                players_max INTEGER DEFAULT 0,
                motd TEXT,
                version TEXT,
                priority INTEGER DEFAULT 2,
                last_seen TIMESTAMP,
                consecutive_failures INTEGER DEFAULT 0,
                whitelist_prob DOUBLE PRECISION DEFAULT 0.0,
                asn TEXT,
                country TEXT,
                favicon TEXT,
                brand TEXT,
                PRIMARY KEY (ip, port)
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS server_players (
                ip TEXT,
                port INTEGER DEFAULT 25565,
                player_name TEXT,
                player_uuid TEXT,
                last_seen TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (ip, port, player_name),
                FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS server_history (
                ip TEXT,
                port INTEGER DEFAULT 25565,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                players_online INTEGER,
                FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS asns (
                asn TEXT PRIMARY KEY,
                org TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'unknown',
                country TEXT,
                tags TEXT,
                last_updated TIMESTAMPTZ
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS asn_ranges (
                cidr TEXT PRIMARY KEY,
                asn TEXT NOT NULL,
                scan_offset BIGINT DEFAULT 0,
                last_scanned_at TIMESTAMP,
                FOREIGN KEY (asn) REFERENCES asns(asn) ON DELETE CASCADE
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS daily_stats (
                date DATE PRIMARY KEY,
                scans_total INTEGER DEFAULT 0,
                scans_hot INTEGER DEFAULT 0,
                scans_warm INTEGER DEFAULT 0,
                scans_cold INTEGER DEFAULT 0,
                discoveries INTEGER DEFAULT 0
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS users (
                id SERIAL PRIMARY KEY,
                name TEXT,
                email TEXT UNIQUE,
                "emailVerified" TIMESTAMPTZ,
                image TEXT,
                role TEXT NOT NULL DEFAULT 'user'
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id SERIAL PRIMARY KEY,
                "userId" INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                type TEXT NOT NULL,
                provider TEXT NOT NULL,
                "providerAccountId" TEXT NOT NULL,
                refresh_token TEXT,
                access_token TEXT,
                expires_at BIGINT,
                token_type TEXT,
                scope TEXT,
                id_token TEXT,
                session_state TEXT,
                UNIQUE(provider, "providerAccountId")
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id SERIAL PRIMARY KEY,
                "sessionToken" TEXT NOT NULL UNIQUE,
                "userId" INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                expires TIMESTAMPTZ NOT NULL
            )
        "#).await?;

        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS verification_token (
                identifier TEXT NOT NULL,
                token TEXT NOT NULL,
                expires TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (identifier, token)
            )
        "#).await?;

        // Indexes
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_status ON servers(status)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_priority ON servers(priority, last_seen)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_asn ON servers(asn)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_asn_status ON servers(asn, status) INCLUDE (ip)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_servers_players ON servers(players_online)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_asns_category ON asns(category)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_asn_ranges_asn ON asn_ranges(asn)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_player_name ON server_players(player_name)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS idx_server_history_ip_port ON server_history(ip, port)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS trgm_idx_servers_ip ON servers USING GIN (ip gin_trgm_ops)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS trgm_idx_servers_motd ON servers USING GIN (motd gin_trgm_ops)").await?;
        db.execute_unprepared("CREATE INDEX IF NOT EXISTS trgm_idx_servers_version ON servers USING GIN (version gin_trgm_ops)").await?;

        // Materialized Views
        db.execute_unprepared(r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS asn_stats AS
            SELECT a.asn, a.org, a.category, a.country, a.tags, a.last_updated,
                   COUNT(s.ip)::bigint as server_count
            FROM asns a
            LEFT JOIN servers s ON s.asn = a.asn AND s.status != 'ignored'
            GROUP BY a.asn
        "#).await?;
        db.execute_unprepared("CREATE UNIQUE INDEX IF NOT EXISTS idx_asn_stats_asn ON asn_stats(asn)").await?;

        db.execute_unprepared(r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS global_stats AS
            SELECT 
                (SELECT COUNT(*)::bigint FROM servers WHERE status != 'ignored') as server_count,
                (SELECT COUNT(*)::bigint FROM servers WHERE status = 'online') as online_count,
                (SELECT COALESCE(SUM(players_online), 0)::bigint FROM servers WHERE status = 'online') as total_players,
                1 as id
        "#).await?;
        db.execute_unprepared("CREATE UNIQUE INDEX IF NOT EXISTS idx_global_stats_id ON global_stats(id)").await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS global_stats").await?;
        db.execute_unprepared("DROP MATERIALIZED VIEW IF EXISTS asn_stats").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS verification_token").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS sessions").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS accounts").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS users").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS daily_stats").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS asn_ranges").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS asns").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS server_history").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS server_players").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS servers").await?;

        Ok(())
    }
}
