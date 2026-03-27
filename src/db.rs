//! Database module with PostgreSQL.
//!
//! Stores server information with priority-based scheduling support.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use thiserror::Error;

use crate::asn::{AsnCategory, AsnRecord};

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database error: {0}")]
    SqlxError(#[from] sqlx::Error),
    #[error("SeaORM error: {0}")]
    SeaOrmError(#[from] sea_orm::DbErr),
}

/// Server status record from database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Server {
    pub ip: String,
    pub port: i32,
    pub server_type: String,
    pub status: String,
    pub players_online: i32,
    pub players_max: i32,
    pub motd: Option<String>,
    pub version: Option<String>,
    pub priority: i32,
    pub last_seen: Option<NaiveDateTime>,
    pub consecutive_failures: i32,
    pub whitelist_prob: f64,
    pub asn: Option<String>,
    pub country: Option<String>,
    pub favicon: Option<String>,
    pub brand: Option<String>,
}

/// ASN record from database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AsnRow {
    pub asn: String,
    pub org: String,
    pub category: String,
    pub country: Option<String>,
    pub last_updated: Option<DateTime<Utc>>,
    pub tags: Option<String>,
    #[sqlx(default)]
    pub server_count: i64,
}

/// ASN range record from database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AsnRangeRow {
    pub cidr: String,
    pub asn: String,
    pub scan_offset: i64,
    pub last_scanned_at: Option<NaiveDateTime>,
}

/// Database wrapper with connection pool.
pub struct Database {
    pool: PgPool,
    sea_db: DatabaseConnection,
}

impl Database {
    /// Create a new database connection
    pub async fn new(db_url: &str) -> Result<Self, DatabaseError> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(30))
            .connect(db_url)
            .await?;

        Self::init_schema(&pool).await?;
        let sea_db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

        tracing::info!("Database initialized connected to postgres");
        Ok(Self { pool, sea_db })
    }

    /// Initialize database schema and run migrations if needed.
    async fn init_schema(pool: &PgPool) -> Result<(), DatabaseError> {
        Self::create_tables(pool).await?;
        Ok(())
    }

    async fn create_tables(pool: &PgPool) -> Result<(), DatabaseError> {
        // Trigram extension for fast text search
        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm").execute(pool).await?;

        sqlx::query(
            r#"
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
            "#
        )
        .execute(pool)
        .await?;

        // Player tracking
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS server_players (
                ip TEXT,
                port INTEGER DEFAULT 25565,
                player_name TEXT,
                player_uuid TEXT,
                last_seen TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (ip, port, player_name),
                FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE
            )
            "#
        )
        .execute(pool)
        .await?;

        // Historical player count
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS server_history (
                ip TEXT,
                port INTEGER DEFAULT 25565,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                players_online INTEGER,
                FOREIGN KEY (ip, port) REFERENCES servers(ip, port) ON DELETE CASCADE
            )
            "#
        )
        .execute(pool)
        .await?;

        // ASN tables for dynamic provider detection
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS asns (
                asn TEXT PRIMARY KEY,
                org TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'unknown',
                country TEXT,
                tags TEXT,
                last_updated TIMESTAMPTZ
            )
            "#
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS asn_ranges (
                cidr TEXT PRIMARY KEY,
                asn TEXT NOT NULL,
                scan_offset BIGINT DEFAULT 0,
                last_scanned_at TIMESTAMP,
                FOREIGN KEY (asn) REFERENCES asns(asn) ON DELETE CASCADE
            )
            "#
        )
        .execute(pool)
        .await?;

        // Daily statistics table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS daily_stats (
                date DATE PRIMARY KEY,
                scans_total INTEGER DEFAULT 0,
                scans_hot INTEGER DEFAULT 0,
                scans_warm INTEGER DEFAULT 0,
                scans_cold INTEGER DEFAULT 0,
                discoveries INTEGER DEFAULT 0
            )
            "#
        )
        .execute(pool)
        .await?;

        // PERFORMANCE INDEXES
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_servers_status ON servers(status)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_servers_priority ON servers(priority, last_seen)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_servers_asn ON servers(asn)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_servers_asn_status ON servers(asn, status) INCLUDE (ip)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_servers_players ON servers(players_online)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_asns_category ON asns(category)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_asn_ranges_asn ON asn_ranges(asn)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_player_name ON server_players(player_name)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_server_history_ip_port ON server_history(ip, port)").execute(pool).await?;

        // Trigram indexes for fast search
        sqlx::query("CREATE INDEX IF NOT EXISTS trgm_idx_servers_ip ON servers USING GIN (ip gin_trgm_ops)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS trgm_idx_servers_motd ON servers USING GIN (motd gin_trgm_ops)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS trgm_idx_servers_version ON servers USING GIN (version gin_trgm_ops)").execute(pool).await?;

        // Auth.js Tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id SERIAL PRIMARY KEY,
                name TEXT,
                email TEXT UNIQUE,
                "emailVerified" TIMESTAMPTZ,
                image TEXT,
                role TEXT NOT NULL DEFAULT 'user'
            )
            "#
        ).execute(pool).await?;

        sqlx::query(
            r#"
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
            "#
        ).execute(pool).await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id SERIAL PRIMARY KEY,
                "sessionToken" TEXT NOT NULL UNIQUE,
                "userId" INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                expires TIMESTAMPTZ NOT NULL
            )
            "#
        ).execute(pool).await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS verification_token (
                identifier TEXT NOT NULL,
                token TEXT NOT NULL,
                expires TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (identifier, token)
            )
            "#
        ).execute(pool).await?;

        // Create Materialized View for fast ASN stats
        sqlx::query(
            r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS asn_stats AS
            SELECT a.asn, a.org, a.category, a.country, a.tags, a.last_updated,
                   COUNT(s.ip)::bigint as server_count
            FROM asns a
            LEFT JOIN servers s ON s.asn = a.asn AND s.status != 'ignored'
            GROUP BY a.asn
            "#
        ).execute(pool).await?;

        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_asn_stats_asn ON asn_stats(asn)").execute(pool).await?;

        // Create Materialized View for fast global stats
        sqlx::query(
            r#"
            CREATE MATERIALIZED VIEW IF NOT EXISTS global_stats AS
            SELECT 
                (SELECT COUNT(*)::bigint FROM servers WHERE status != 'ignored') as server_count,
                (SELECT COUNT(*)::bigint FROM servers WHERE status = 'online') as online_count,
                (SELECT COALESCE(SUM(players_online), 0)::bigint FROM servers WHERE status = 'online') as total_players,
                1 as id
            "#
        ).execute(pool).await?;
        
        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_global_stats_id ON global_stats(id)").execute(pool).await?;

        Ok(())
    }

    /// Refresh materialized views
    pub async fn refresh_materialized_views(&self) -> Result<(), DatabaseError> {
        sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY asn_stats").execute(&self.pool).await?;
        sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY global_stats").execute(&self.pool).await?;
        Ok(())
    }

    /// Get the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Insert or update a server record.
    pub async fn upsert_server(&self, server: &Server) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO servers (ip, port, server_type, status, players_online, players_max, motd, version, 
                                priority, last_seen, consecutive_failures, whitelist_prob, asn, country, favicon, brand)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ON CONFLICT(ip, port) DO UPDATE SET
                server_type = excluded.server_type,
                status = excluded.status,
                players_online = excluded.players_online,
                players_max = excluded.players_max,
                motd = excluded.motd,
                version = excluded.version,
                priority = excluded.priority,
                last_seen = excluded.last_seen,
                consecutive_failures = excluded.consecutive_failures,
                whitelist_prob = excluded.whitelist_prob,
                asn = excluded.asn,
                country = excluded.country,
                favicon = COALESCE(excluded.favicon, servers.favicon),
                brand = COALESCE(excluded.brand, servers.brand)
            "#,
        )
        .bind(&server.ip)
        .bind(server.port)
        .bind(&server.server_type)
        .bind(&server.status)
        .bind(server.players_online)
        .bind(server.players_max)
        .bind(&server.motd)
        .bind(&server.version)
        .bind(server.priority)
        .bind(server.last_seen)
        .bind(server.consecutive_failures)
        .bind(server.whitelist_prob)
        .bind(&server.asn)
        .bind(&server.country)
        .bind(&server.favicon)
        .bind(&server.brand)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a server by IP and port.
    pub async fn get_server(&self, ip: &str, port: i32) -> Result<Option<Server>, DatabaseError> {
        let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE ip = $1 AND port = $2")
            .bind(ip)
            .bind(port)
            .fetch_optional(&self.pool)
            .await?;
        Ok(server)
    }

    /// Get servers ordered by priority (for scheduler).
    pub async fn get_servers_by_priority(&self, limit: i32) -> Result<Vec<Server>, DatabaseError> {
        let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers ORDER BY priority ASC, last_seen ASC LIMIT $1")
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(servers)
    }

    /// Get online servers ordered by player count (for API).
    pub async fn get_online_servers(&self, limit: i32) -> Result<Vec<Server>, DatabaseError> {
        let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE status = 'online' ORDER BY players_online DESC LIMIT $1")
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(servers)
    }

    pub async fn get_all_servers(
        &self,
        status_filter: Option<&str>,
        search_query: Option<&str>,
        limit: i32,
        min_players: Option<i32>,
        max_players: Option<i32>,
        version: Option<&str>,
        asn_category: Option<&str>,
        whitelist_prob_min: Option<f64>,
        country: Option<&str>,
        brand: Option<&str>,
        server_type_filter: Option<&str>,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        cursor_players: Option<i32>,
        cursor_ip: Option<&str>,
        cursor_last_seen: Option<NaiveDateTime>,
        asn_filter: Option<&str>,
    ) -> Result<Vec<Server>, DatabaseError> {
        let mut query = String::from("SELECT * FROM servers WHERE status != 'ignored'");
        
        if let Some(status) = status_filter {
            if status != "all" {
                let safe_status = status.replace("'", "''");
                query.push_str(&format!(" AND status = '{}'", safe_status));
            }
        }

        if let Some(st) = server_type_filter {
            let safe_st = st.replace("'", "''");
            query.push_str(&format!(" AND server_type = '{}'", safe_st));
        }

        if let Some(search) = search_query {
            let safe_search = search.replace("'", "''");
            query.push_str(&format!(" AND (ip ILIKE '%{}%' OR motd ILIKE '%{}%' OR version ILIKE '%{}%')", safe_search, safe_search, safe_search));
        }
        
        if let Some(min_p) = min_players {
            query.push_str(&format!(" AND players_online >= {}", min_p));
        }
        
        if let Some(max_p) = max_players {
            query.push_str(&format!(" AND players_online <= {}", max_p));
        }
        
        if let Some(ver) = version {
            let safe_ver = ver.replace("'", "''");
            query.push_str(&format!(" AND version ILIKE '%{}%'", safe_ver));
        }
        
        if let Some(prob) = whitelist_prob_min {
            query.push_str(&format!(" AND whitelist_prob >= {}", prob));
        }

        if let Some(cat) = asn_category {
            let safe_cat = cat.replace("'", "''");
            query.push_str(&format!(" AND asn IN (SELECT asn FROM asns WHERE category = '{}')", safe_cat));
        }

        if let Some(asn) = asn_filter {
            let safe_asn = asn.replace("'", "''");
            query.push_str(&format!(" AND asn = '{}'", safe_asn));
        }
        
        if let Some(c) = country {
            let safe_c = c.replace("'", "''");
            query.push_str(&format!(" AND country = '{}'", safe_c));
        }

        if let Some(b) = brand {
            let safe_b = b.replace("'", "''");
            query.push_str(&format!(" AND brand = '{}'", safe_b));
        }

        let sort_col = match sort_by {
            Some("players") => "players_online",
            Some("last_seen") => "last_seen",
            Some("ip") => "ip",
            _ => "players_online",
        };

        let order = match sort_order {
            Some("asc") => "ASC",
            _ => "DESC",
        };

        // Pagination: Cursor-based
        if let Some(c_ip) = cursor_ip {
            let safe_c_ip = c_ip.replace("'", "''");
            match sort_col {
                "players_online" => {
                    if let Some(c_val) = cursor_players {
                        if order == "DESC" {
                            query.push_str(&format!(" AND (players_online < {} OR (players_online = {} AND ip > '{}'))", c_val, c_val, safe_c_ip));
                        } else {
                            query.push_str(&format!(" AND (players_online > {} OR (players_online = {} AND ip > '{}'))", c_val, c_val, safe_c_ip));
                        }
                    }
                }
                "last_seen" => {
                    if let Some(c_val) = cursor_last_seen {
                        let c_val_str = c_val.format("%Y-%m-%d %H:%M:%S").to_string();
                        if order == "DESC" {
                            query.push_str(&format!(" AND (last_seen < '{}' OR (last_seen = '{}' AND ip > '{}'))", c_val_str, c_val_str, safe_c_ip));
                        } else {
                            query.push_str(&format!(" AND (last_seen > '{}' OR (last_seen = '{}' AND ip > '{}'))", c_val_str, c_val_str, safe_c_ip));
                        }
                    }
                }
                "ip" => {
                    if order == "DESC" {
                        query.push_str(&format!(" AND ip < '{}'", safe_c_ip));
                    } else {
                        query.push_str(&format!(" AND ip > '{}'", safe_c_ip));
                    }
                }
                _ => {}
            }
        }

        query.push_str(&format!(" ORDER BY {} {}, ip ASC LIMIT {}", sort_col, order, limit));

        let servers = sqlx::query_as::<_, Server>(&query)
            .fetch_all(&self.pool)
            .await?;
        Ok(servers)
    }

    /// Get players for a specific server
    pub async fn get_server_players(&self, ip: &str, port: i32) -> Result<Vec<(String, String, NaiveDateTime)>, DatabaseError> {
        let rows = sqlx::query_as::<_, (String, String, NaiveDateTime)>(
            "SELECT player_name, player_uuid, last_seen FROM server_players WHERE ip = $1 AND port = $2 ORDER BY last_seen DESC LIMIT 100"
        )
        .bind(ip)
        .bind(port)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get player count history for a server
    pub async fn get_server_history(&self, ip: &str, port: i32, limit: i32) -> Result<Vec<(NaiveDateTime, i32)>, DatabaseError> {
        let rows = sqlx::query_as::<_, (NaiveDateTime, i32)>(
            "SELECT timestamp, players_online FROM server_history WHERE ip = $1 AND port = $2 ORDER BY timestamp DESC LIMIT $3"
        )
        .bind(ip)
        .bind(port)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().rev().collect()) // Return in chronological order
    }

    /// Search for a player by name
    pub async fn search_players(&self, name: &str) -> Result<Vec<(String, i32, String, NaiveDateTime)>, DatabaseError> {
        let safe_name = name.replace("'", "''");
        let query = format!(
            "SELECT ip, port, player_name, last_seen FROM server_players WHERE player_name ILIKE '%{}%' ORDER BY last_seen DESC LIMIT 50",
            safe_name
        );
        let rows = sqlx::query_as::<_, (String, i32, String, NaiveDateTime)>(&query)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Get server count.
    pub async fn get_server_count(&self) -> Result<i64, DatabaseError> {
        let count: Option<i64> = sqlx::query_scalar("SELECT server_count FROM global_stats WHERE id = 1").fetch_one(&self.pool).await?;
        Ok(count.unwrap_or(0))
    }

    /// Get online server count.
    pub async fn get_online_count(&self) -> Result<i64, DatabaseError> {
        let count: Option<i64> = sqlx::query_scalar("SELECT online_count FROM global_stats WHERE id = 1").fetch_one(&self.pool).await?;
        Ok(count.unwrap_or(0))
    }

    /// Get total players online.
    pub async fn get_total_players(&self) -> Result<i64, DatabaseError> {
        let count: Option<i64> = sqlx::query_scalar("SELECT total_players FROM global_stats WHERE id = 1").fetch_one(&self.pool).await?;
        Ok(count.unwrap_or(0))
    }

    /// Update server status to online with priority reset. Returns whether it was a new discovery.
    pub async fn mark_online(
        &self,
        ip: &str,
        port: i32,
        server_type: &str,
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
        players_sample: Option<Vec<crate::slp::PlayerSample>>,
        favicon: Option<String>,
        brand: Option<String>,
        asn_record: Option<crate::asn::AsnRecord>,
    ) -> Result<bool, DatabaseError> {
        let mut retries = 3;
        while retries > 0 {
            match self.mark_online_inner(ip, port, server_type, players_online, players_max, motd.clone(), version.clone(), players_sample.clone(), favicon.clone(), brand.clone(), asn_record.clone()).await {
                Ok(is_new) => return Ok(is_new),
                Err(_e) if retries > 1 => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    retries -= 1;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    }

    async fn mark_online_inner(
        &self,
        ip: &str,
        port: i32,
        server_type: &str,
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
        players_sample: Option<Vec<crate::slp::PlayerSample>>,
        favicon: Option<String>,
        brand: Option<String>,
        asn_record: Option<crate::asn::AsnRecord>,
    ) -> Result<bool, DatabaseError> {
        let existing = self.get_server(ip, port).await?;
        let is_new = existing.is_none();

        let (asn, country) = if let Some(record) = asn_record {
            (Some(record.asn), record.country)
        } else {
            (None, None)
        };

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO servers (
                ip, port, server_type, status, players_online, players_max, motd, version, 
                priority, last_seen, consecutive_failures, asn, country, favicon, brand
            )
            VALUES ($1, $2, $3, 'online', $4, $5, $6, $7, 1, CURRENT_TIMESTAMP, 0, $8, $9, $10, $11)
            ON CONFLICT(ip, port) DO UPDATE SET
                server_type = excluded.server_type,
                status = 'online',
                players_online = excluded.players_online,
                players_max = excluded.players_max,
                motd = excluded.motd,
                version = excluded.version,
                priority = 1,
                last_seen = CURRENT_TIMESTAMP,
                consecutive_failures = 0,
                asn = COALESCE(servers.asn, excluded.asn),
                country = COALESCE(servers.country, excluded.country),
                favicon = COALESCE(excluded.favicon, servers.favicon),
                brand = COALESCE(excluded.brand, servers.brand)
            "#,
        )
        .bind(ip).bind(port).bind(server_type).bind(players_online).bind(players_max).bind(&motd).bind(&version).bind(asn).bind(country).bind(favicon).bind(brand)
        .execute(&mut *tx).await?;

        sqlx::query("INSERT INTO server_history (ip, port, players_online) VALUES ($1, $2, $3)").bind(ip).bind(port).bind(players_online).execute(&mut *tx).await?;

        if let Some(sample) = players_sample {
            for player in sample {
                let name = player.name.trim();
                if !name.is_empty() {
                    sqlx::query(
                        r#"
                        INSERT INTO server_players (ip, port, player_name, player_uuid, last_seen)
                        VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
                        ON CONFLICT(ip, port, player_name) DO UPDATE SET
                            player_uuid = excluded.player_uuid,
                            last_seen = CURRENT_TIMESTAMP
                        "#
                    ).bind(ip).bind(port).bind(name).bind(&player.id).execute(&mut *tx).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(is_new)
    }

    pub async fn mark_offline(
        &self,
        ip: &str,
        port: i32,
        server_type: Option<&str>,
        asn_record: Option<crate::asn::AsnRecord>,
    ) -> Result<(), DatabaseError> {
        let (asn, country) = if let Some(record) = asn_record {
            (Some(record.asn), record.country)
        } else {
            (None, None)
        };

        sqlx::query(
            r#"
            INSERT INTO servers (ip, port, server_type, status, priority, last_seen, consecutive_failures, asn, country)
            VALUES ($1, $2, $3, 'ignored', 3, CURRENT_TIMESTAMP, 1, $4, $5)
            ON CONFLICT(ip, port) DO UPDATE SET
                status = CASE 
                    WHEN servers.motd IS NOT NULL OR servers.status = 'online' THEN 'offline' 
                    ELSE 'ignored' 
                END,
                consecutive_failures = servers.consecutive_failures + 1,
                last_seen = CURRENT_TIMESTAMP,
                priority = CASE WHEN servers.consecutive_failures >= 5 THEN 3 ELSE servers.priority END,
                asn = COALESCE(servers.asn, excluded.asn),
                country = COALESCE(servers.country, excluded.country)
            "#,
        )
        .bind(ip)
        .bind(port)
        .bind(server_type.unwrap_or("java"))
        .bind(asn)
        .bind(country)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_server_if_new(&self, ip: &str, port: i32, server_type: &str) -> Result<(), DatabaseError> {
        sqlx::query("INSERT INTO servers (ip, port, server_type) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING").bind(ip).bind(port).bind(server_type).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_asn(&self, asn: &str, org: &str, category: &str, country: Option<&str>, tags: Option<Vec<String>>) -> Result<(), DatabaseError> {
        let tags_str = tags.map(|t| t.join(","));
        sqlx::query(
            r#"
            INSERT INTO asns (asn, org, category, country, tags, last_updated)
            VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP)
            ON CONFLICT(asn) DO UPDATE SET
                org = excluded.org, 
                category = excluded.category, 
                country = COALESCE(excluded.country, asns.country), 
                tags = COALESCE(excluded.tags, asns.tags),
                last_updated = CURRENT_TIMESTAMP
            "#,
        ).bind(asn).bind(org).bind(category).bind(country).bind(tags_str).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_asn_range(&self, cidr: &str, asn: &str) -> Result<(), DatabaseError> {
        sqlx::query("INSERT INTO asn_ranges (cidr, asn) VALUES ($1, $2) ON CONFLICT(cidr) DO UPDATE SET asn = excluded.asn").bind(cidr).bind(asn).execute(&self.pool).await?;
        Ok(())
    }

    fn map_asn_row(&self, row: AsnRow) -> AsnRecord {
        AsnRecord {
            asn: row.asn,
            org: row.org,
            category: match row.category.as_str() {
                "hosting" => AsnCategory::Hosting,
                "residential" => AsnCategory::Residential,
                "excluded" => AsnCategory::Excluded,
                _ => AsnCategory::Unknown,
            },
            country: row.country,
            last_updated: row.last_updated,
            server_count: row.server_count,
            tags: row.tags.map(|t| t.split(',').map(|s| s.to_string()).collect()).unwrap_or_default(),
        }
    }

    pub async fn get_all_asns(&self) -> Result<Vec<AsnRecord>, DatabaseError> {
        let rows = sqlx::query_as::<_, AsnRow>("SELECT * FROM asns").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|row| self.map_asn_row(row)).collect())
    }

    pub async fn get_all_asn_ranges(&self) -> Result<Vec<AsnRangeRow>, DatabaseError> {
        sqlx::query_as::<_, AsnRangeRow>("SELECT * FROM asn_ranges").fetch_all(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn get_asns_by_category(&self, category: &str) -> Result<Vec<AsnRecord>, DatabaseError> {
        let rows = sqlx::query_as::<_, AsnRow>("SELECT * FROM asns WHERE category = $1 ORDER BY org").bind(category).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|row| self.map_asn_row(row)).collect())
    }

    pub async fn get_stale_asns(&self, days: i64) -> Result<Vec<AsnRecord>, DatabaseError> {
        let interval_str = format!("{} days", days);
        let rows = sqlx::query_as::<_, AsnRow>("SELECT * FROM asns WHERE last_updated IS NULL OR last_updated < NOW() - CAST($1 AS INTERVAL) ORDER BY last_updated ASC").bind(interval_str).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|row| self.map_asn_row(row)).collect())
    }

    pub async fn get_asn_list_with_counts(&self) -> Result<Vec<AsnRecord>, DatabaseError> {
        let rows = sqlx::query_as::<_, AsnRow>(
            r#"
            SELECT asn, org, category, country, tags, last_updated, server_count
            FROM asn_stats
            ORDER BY server_count DESC, org ASC
            "#
        ).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|row| self.map_asn_row(row)).collect())
    }

    pub async fn get_asn_list_paginated(&self, page: u64, limit: u64) -> Result<(Vec<AsnRecord>, i64), DatabaseError> {
        let offset = page * limit;
        
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM asn_stats")
            .fetch_one(&self.pool).await?;

        let rows = sqlx::query_as::<_, AsnRow>(
            r#"
            SELECT asn, org, category, country, tags, last_updated, server_count
            FROM asn_stats
            ORDER BY server_count DESC, org ASC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool).await?;

        Ok((rows.into_iter().map(|row| self.map_asn_row(row)).collect(), total))
    }

    pub async fn get_server_count_by_asn(&self, asn: &str) -> Result<i64, DatabaseError> {
        let count: Option<i64> = sqlx::query_scalar("SELECT server_count FROM asn_stats WHERE asn = $1").bind(asn).fetch_one(&self.pool).await?;
        Ok(count.unwrap_or(0))
    }

    pub async fn get_asn_stats_v2(&self) -> Result<(i64, i64, i64, i64), DatabaseError> {
        let hosting: Option<i64> = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM asns WHERE category = 'hosting'").fetch_one(&self.pool).await?;
        let residential: Option<i64> = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM asns WHERE category = 'residential'").fetch_one(&self.pool).await?;
        let excluded: Option<i64> = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM asns WHERE category = 'excluded'").fetch_one(&self.pool).await?;
        let unknown: Option<i64> = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM asns WHERE category = 'unknown'").fetch_one(&self.pool).await?;
        Ok((hosting.unwrap_or(0), residential.unwrap_or(0), excluded.unwrap_or(0), unknown.unwrap_or(0)))
    }

    pub async fn get_asn_count(&self) -> Result<i64, DatabaseError> {
        let count: Option<i64> = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM asns").fetch_one(&self.pool).await?;
        Ok(count.unwrap_or(0))
    }

    pub async fn get_hosting_ranges(&self) -> Result<Vec<(String, String)>, DatabaseError> {
        sqlx::query_as::<_, (String, String)>("SELECT r.cidr, r.asn FROM asn_ranges r JOIN asns a ON r.asn = a.asn WHERE a.category = 'hosting' ORDER BY a.org").fetch_all(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn get_next_range_to_scan(&self, category: &str) -> Result<Option<AsnRangeRow>, DatabaseError> {
        sqlx::query_as::<_, AsnRangeRow>("SELECT r.* FROM asn_ranges r JOIN asns a ON r.asn = a.asn WHERE a.category = $1 ORDER BY r.last_scanned_at ASC NULLS FIRST, r.scan_offset ASC LIMIT 1").bind(category).fetch_optional(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn get_ranges_to_scan(&self, category: &str, limit: i32) -> Result<Vec<AsnRangeRow>, DatabaseError> {
        sqlx::query_as::<_, AsnRangeRow>("SELECT r.* FROM asn_ranges r JOIN asns a ON r.asn = a.asn WHERE a.category = $1 ORDER BY r.last_scanned_at ASC NULLS FIRST, r.scan_offset ASC LIMIT $2").bind(category).bind(limit as i64).fetch_all(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn update_range_progress(&self, cidr: &str, new_offset: i64, reset: bool) -> Result<(), DatabaseError> {
        if reset { sqlx::query("UPDATE asn_ranges SET scan_offset = 0, last_scanned_at = CURRENT_TIMESTAMP WHERE cidr = $1").bind(cidr).execute(&self.pool).await?; }
        else { sqlx::query("UPDATE asn_ranges SET scan_offset = $1 WHERE cidr = $2").bind(new_offset).bind(cidr).execute(&self.pool).await?; }
        Ok(())
    }

    pub async fn update_batch_range_progress(&self, updates: Vec<(String, i64, bool)>) -> Result<(), DatabaseError> {
        let mut tx = self.pool.begin().await?;
        for (cidr, offset, reset) in updates {
            if reset { sqlx::query("UPDATE asn_ranges SET scan_offset = 0, last_scanned_at = CURRENT_TIMESTAMP WHERE cidr = $1").bind(&cidr).execute(&mut *tx).await?; }
            else { sqlx::query("UPDATE asn_ranges SET scan_offset = $1 WHERE cidr = $2").bind(offset).bind(&cidr).execute(&mut *tx).await?; }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn increment_stats(&self, tier: i32, found_new: bool) -> Result<(), DatabaseError> {
        let date = Utc::now().date_naive();
        let tier_col = match tier { 1 => "scans_hot", 2 => "scans_warm", _ => "scans_cold" };
        let found_val = if found_new { 1 } else { 0 };
        let query = format!("INSERT INTO daily_stats (date, scans_total, {}, discoveries) VALUES ($1, 1, 1, $2) ON CONFLICT(date) DO UPDATE SET scans_total = daily_stats.scans_total + 1, {} = daily_stats.{} + 1, discoveries = daily_stats.discoveries + $3", tier_col, tier_col, tier_col);
        sqlx::query(&query).bind(date).bind(found_val).bind(found_val).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn increment_batch_stats(&self, hot: i32, warm: i32, cold: i32, discoveries: i32) -> Result<(), DatabaseError> {
        if hot == 0 && warm == 0 && cold == 0 && discoveries == 0 { return Ok(()); }
        let date = Utc::now().date_naive();
        let total = hot + warm + cold;
        sqlx::query("INSERT INTO daily_stats (date, scans_total, scans_hot, scans_warm, scans_cold, discoveries) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT(date) DO UPDATE SET scans_total = daily_stats.scans_total + EXCLUDED.scans_total, scans_hot = daily_stats.scans_hot + EXCLUDED.scans_hot, scans_warm = daily_stats.scans_warm + EXCLUDED.scans_warm, scans_cold = daily_stats.scans_cold + EXCLUDED.scans_cold, discoveries = daily_stats.discoveries + EXCLUDED.discoveries").bind(date).bind(total).bind(hot).bind(warm).bind(cold).bind(discoveries).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn link_servers_to_asns(&self) -> Result<u64, DatabaseError> {
        let ranges = self.get_all_asn_ranges().await?;
        let asns = self.get_all_asns().await?;
        let asn_map: std::collections::HashMap<String, (String, Option<String>)> = asns.into_iter().map(|a| (a.asn, (a.org, a.country))).collect();
        let mut linked = 0;
        for range in ranges {
            if let Some((_org, country)) = asn_map.get(&range.asn) {
                let like_str = range.cidr.replace("/32", "").replace("/24", "%").replace("/16", ".%");
                let result = sqlx::query("UPDATE servers SET asn = $1, country = COALESCE(country, $2) WHERE asn IS NULL AND ip LIKE $3").bind(&range.asn).bind(country).bind(like_str).execute(&self.pool).await?;
                linked += result.rows_affected();
            }
        }
        Ok(linked)
    }
}
