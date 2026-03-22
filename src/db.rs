//! Database module with SQLite and WAL mode.
//!
//! Stores server information with priority-based scheduling support.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use crate::asn::{AsnCategory, AsnRecord};

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database error: {0}")]
    SqlxError(#[from] sqlx::Error),
}

/// Server status record from database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Server {
    pub ip: String,
    pub port: i32,
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
}

/// ASN record from database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AsnRow {
    pub asn: String,
    pub org: String,
    pub category: String,
    pub country: Option<String>,
    pub last_updated: Option<DateTime<Utc>>,
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
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection with WAL mode enabled.
    pub async fn new(db_path: &str) -> Result<Self, DatabaseError> {
        // Ensure proper SQLite URL format
        let mut url = if db_path.starts_with("sqlite:") || db_path.starts_with("file:") {
            db_path.to_string()
        } else if db_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}", db_path)
        };

        // Ensure the file is created if it doesn't exist
        if !url.contains('?') {
            url.push_str("?mode=rwc");
        } else if !url.contains("mode=rwc") {
            url.push_str("&mode=rwc");
        }
        
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&url)
            .await?;

        // Enable WAL mode and optimize settings
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA synchronous = NORMAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA cache_size = -64000") // 64MB cache
            .execute(&pool)
            .await?;

        // Create tables
        Self::init_schema(&pool).await?;

        tracing::info!("Database initialized at {}", db_path);
        Ok(Self { pool })
    }

    /// Initialize database schema.
    async fn init_schema(pool: &SqlitePool) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS servers (
                ip TEXT PRIMARY KEY,
                port INTEGER DEFAULT 25565,
                status TEXT DEFAULT 'unknown',
                players_online INTEGER DEFAULT 0,
                players_max INTEGER DEFAULT 0,
                motd TEXT,
                version TEXT,
                priority INTEGER DEFAULT 2,
                last_seen TIMESTAMP,
                consecutive_failures INTEGER DEFAULT 0,
                whitelist_prob REAL DEFAULT 0.0,
                asn TEXT,
                country TEXT
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
                player_name TEXT,
                player_uuid TEXT,
                last_seen TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (ip, player_name),
                FOREIGN KEY (ip) REFERENCES servers(ip)
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
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                players_online INTEGER,
                FOREIGN KEY (ip) REFERENCES servers(ip)
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
                last_updated TIMESTAMP
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
                scan_offset INTEGER DEFAULT 0,
                last_scanned_at TIMESTAMP,
                FOREIGN KEY (asn) REFERENCES asns(asn)
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

        // Indexes for performance
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_priority ON servers(priority)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_status ON servers(status)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_asn_category ON asns(category)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_asn_ranges_asn ON asn_ranges(asn)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_player_name ON server_players(player_name)").execute(pool).await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_server_history_ip ON server_history(ip)").execute(pool).await?;

        // Attempt to add new columns if table already exists (ignore errors if they already exist)
        let _ = sqlx::query("ALTER TABLE servers ADD COLUMN asn TEXT;").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE servers ADD COLUMN country TEXT;").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE asn_ranges ADD COLUMN scan_offset INTEGER DEFAULT 0;").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE asn_ranges ADD COLUMN last_scanned_at TIMESTAMP;").execute(pool).await;

        Ok(())
    }

    /// Get the connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Insert or update a server record.
    pub async fn upsert_server(&self, server: &Server) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO servers (ip, port, status, players_online, players_max, motd, version, 
                                priority, last_seen, consecutive_failures, whitelist_prob, asn, country)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(ip) DO UPDATE SET
                port = excluded.port,
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
                country = excluded.country
            "#,
        )
        .bind(&server.ip)
        .bind(server.port)
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a server by IP.
    pub async fn get_server(&self, ip: &str) -> Result<Option<Server>, DatabaseError> {
        let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE ip = ?")
            .bind(ip)
            .fetch_optional(&self.pool)
            .await?;
        Ok(server)
    }

    /// Get servers ordered by priority (for scheduler).
    pub async fn get_servers_by_priority(&self, limit: i32) -> Result<Vec<Server>, DatabaseError> {
        let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers ORDER BY priority ASC, last_seen ASC LIMIT ?")
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(servers)
    }

    /// Get online servers ordered by player count (for API).
    pub async fn get_online_servers(&self, limit: i32) -> Result<Vec<Server>, DatabaseError> {
        let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE status = 'online' ORDER BY players_online DESC LIMIT ?")
            .bind(limit)
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
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        cursor_players: Option<i32>,
        cursor_ip: Option<&str>,
        cursor_last_seen: Option<NaiveDateTime>,
    ) -> Result<Vec<Server>, DatabaseError> {
        let mut query = String::from("SELECT * FROM servers WHERE 1=1");
        
        if let Some(status) = status_filter {
            query.push_str(&format!(" AND status = '{}'", status));
        }

        if let Some(search) = search_query {
            let safe_search = search.replace("'", "''");
            query.push_str(&format!(" AND (ip LIKE '%{}%' OR motd LIKE '%{}%' OR version LIKE '%{}%')", safe_search, safe_search, safe_search));
        }
        
        if let Some(min_p) = min_players {
            query.push_str(&format!(" AND players_online >= {}", min_p));
        }
        
        if let Some(max_p) = max_players {
            query.push_str(&format!(" AND players_online <= {}", max_p));
        }
        
        if let Some(ver) = version {
            let safe_ver = ver.replace("'", "''");
            query.push_str(&format!(" AND version LIKE '%{}%'", safe_ver));
        }
        
        if let Some(prob) = whitelist_prob_min {
            query.push_str(&format!(" AND whitelist_prob >= {}", prob));
        }

        if let Some(cat) = asn_category {
            let safe_cat = cat.replace("'", "''");
            query.push_str(&format!(" AND asn IN (SELECT asn FROM asns WHERE category = '{}')", safe_cat));
        }
        
        if let Some(c) = country {
            let safe_c = c.replace("'", "''");
            query.push_str(&format!(" AND country = '{}'", safe_c));
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
    pub async fn get_server_players(&self, ip: &str) -> Result<Vec<(String, String, NaiveDateTime)>, DatabaseError> {
        let rows = sqlx::query_as::<_, (String, String, NaiveDateTime)>(
            "SELECT player_name, player_uuid, last_seen FROM server_players WHERE ip = ? ORDER BY last_seen DESC LIMIT 100"
        )
        .bind(ip)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get player count history for a server
    pub async fn get_server_history(&self, ip: &str, limit: i32) -> Result<Vec<(NaiveDateTime, i32)>, DatabaseError> {
        let rows = sqlx::query_as::<_, (NaiveDateTime, i32)>(
            "SELECT timestamp, players_online FROM server_history WHERE ip = ? ORDER BY timestamp DESC LIMIT ?"
        )
        .bind(ip)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Search for a player by name
    pub async fn search_players(&self, name: &str) -> Result<Vec<(String, String, NaiveDateTime)>, DatabaseError> {
        let safe_name = name.replace("'", "''");
        let query = format!(
            "SELECT ip, player_name, last_seen FROM server_players WHERE player_name LIKE '%{}%' COLLATE NOCASE ORDER BY last_seen DESC LIMIT 50",
            safe_name
        );
        let rows = sqlx::query_as::<_, (String, String, NaiveDateTime)>(&query)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Get server count.
    pub async fn get_server_count(&self) -> Result<i64, DatabaseError> {
        let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM servers").fetch_one(&self.pool).await?;
        Ok(count)
    }

    /// Get online server count.
    pub async fn get_online_count(&self) -> Result<i64, DatabaseError> {
        let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM servers WHERE status = 'online'").fetch_one(&self.pool).await?;
        Ok(count)
    }

    /// Get total players online.
    pub async fn get_total_players(&self) -> Result<i64, DatabaseError> {
        let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COALESCE(SUM(players_online), 0) FROM servers WHERE status = 'online'").fetch_one(&self.pool).await?;
        Ok(count)
    }

    /// Update server status to online with priority reset. Returns whether it was a new discovery.
    pub async fn mark_online(
        &self,
        ip: &str,
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
        players_sample: Option<Vec<crate::slp::PlayerSample>>,
        asn_manager: Option<Arc<tokio::sync::RwLock<crate::asn::AsnManager>>>,
    ) -> Result<bool, DatabaseError> {
        let mut retries = 3;
        while retries > 0 {
            match self.mark_online_inner(ip, players_online, players_max, motd.clone(), version.clone(), players_sample.clone(), asn_manager.clone()).await {
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
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
        players_sample: Option<Vec<crate::slp::PlayerSample>>,
        asn_manager: Option<Arc<tokio::sync::RwLock<crate::asn::AsnManager>>>,
    ) -> Result<bool, DatabaseError> {
        let existing = self.get_server(ip).await?;
        let is_new = existing.is_none();

        let (asn, country) = if let (Some(manager_lock), Ok(ip_addr)) = (asn_manager, ip.parse::<std::net::Ipv4Addr>()) {
            let manager: tokio::sync::RwLockReadGuard<'_, crate::asn::AsnManager> = manager_lock.read().await;
            if let Some(record) = manager.get_asn_for_ip(ip_addr) {
                (Some(record.asn.clone()), record.country.clone())
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO servers (ip, port, status, players_online, players_max, motd, version, 
                                priority, last_seen, consecutive_failures, asn, country)
            VALUES (?, 25565, 'online', ?, ?, ?, ?, 1, CURRENT_TIMESTAMP, 0, ?, ?)
            ON CONFLICT(ip) DO UPDATE SET
                status = 'online',
                players_online = excluded.players_online,
                players_max = excluded.players_max,
                motd = excluded.motd,
                version = excluded.version,
                priority = 1,
                last_seen = CURRENT_TIMESTAMP,
                consecutive_failures = 0,
                asn = COALESCE(servers.asn, excluded.asn),
                country = COALESCE(servers.country, excluded.country)
            "#,
        )
        .bind(ip).bind(players_online).bind(players_max).bind(&motd).bind(&version).bind(asn).bind(country)
        .execute(&mut *tx).await?;

        sqlx::query("INSERT INTO server_history (ip, players_online) VALUES (?, ?)").bind(ip).bind(players_online).execute(&mut *tx).await?;

        if let Some(sample) = players_sample {
            for player in sample {
                let name = player.name.trim();
                if !name.is_empty() {
                    sqlx::query(
                        r#"
                        INSERT INTO server_players (ip, player_name, player_uuid, last_seen)
                        VALUES (?, ?, ?, CURRENT_TIMESTAMP)
                        ON CONFLICT(ip, player_name) DO UPDATE SET
                            player_uuid = excluded.player_uuid,
                            last_seen = CURRENT_TIMESTAMP
                        "#
                    ).bind(ip).bind(name).bind(&player.id).execute(&mut *tx).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(is_new)
    }

    pub async fn mark_offline(&self, ip: &str) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            UPDATE servers SET
                status = 'offline',
                consecutive_failures = consecutive_failures + 1,
                last_seen = CURRENT_TIMESTAMP,
                priority = CASE WHEN consecutive_failures >= 5 THEN 3 ELSE priority END
            WHERE ip = ?
            "#,
        )
        .bind(ip).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn insert_server_if_new(&self, ip: &str, port: i32) -> Result<(), DatabaseError> {
        sqlx::query("INSERT OR IGNORE INTO servers (ip, port) VALUES (?, ?)").bind(ip).bind(port).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_asn(&self, asn: &str, org: &str, category: &str, country: Option<&str>) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            INSERT INTO asns (asn, org, category, country, last_updated)
            VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(asn) DO UPDATE SET
                org = excluded.org, category = excluded.category, country = excluded.country, last_updated = CURRENT_TIMESTAMP
            "#,
        ).bind(asn).bind(org).bind(category).bind(country).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn upsert_asn_range(&self, cidr: &str, asn: &str) -> Result<(), DatabaseError> {
        sqlx::query("INSERT INTO asn_ranges (cidr, asn) VALUES (?, ?) ON CONFLICT(cidr) DO UPDATE SET asn = excluded.asn").bind(cidr).bind(asn).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_all_asns(&self) -> Result<Vec<AsnRecord>, DatabaseError> {
        let rows = sqlx::query_as::<_, AsnRow>("SELECT * FROM asns").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|row| AsnRecord {
            asn: row.asn, org: row.org,
            category: match row.category.as_str() { "hosting" => AsnCategory::Hosting, "residential" => AsnCategory::Residential, "excluded" => AsnCategory::Excluded, _ => AsnCategory::Unknown },
            country: row.country, last_updated: row.last_updated,
        }).collect())
    }

    pub async fn get_all_asn_ranges(&self) -> Result<Vec<AsnRangeRow>, DatabaseError> {
        sqlx::query_as::<_, AsnRangeRow>("SELECT * FROM asn_ranges").fetch_all(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn get_asns_by_category(&self, category: &str) -> Result<Vec<AsnRecord>, DatabaseError> {
        let rows = sqlx::query_as::<_, AsnRow>("SELECT * FROM asns WHERE category = ? ORDER BY org").bind(category).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|row| AsnRecord {
            asn: row.asn, org: row.org,
            category: match row.category.as_str() { "hosting" => AsnCategory::Hosting, "residential" => AsnCategory::Residential, "excluded" => AsnCategory::Excluded, _ => AsnCategory::Unknown },
            country: row.country, last_updated: row.last_updated,
        }).collect())
    }

    pub async fn get_stale_asns(&self, days: i64) -> Result<Vec<AsnRecord>, DatabaseError> {
        let rows = sqlx::query_as::<_, AsnRow>("SELECT * FROM asns WHERE last_updated IS NULL OR last_updated < datetime('now', ?) ORDER BY last_updated ASC").bind(&format!("-{} days", days)).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|row| AsnRecord {
            asn: row.asn, org: row.org,
            category: match row.category.as_str() { "hosting" => AsnCategory::Hosting, "residential" => AsnCategory::Residential, "excluded" => AsnCategory::Excluded, _ => AsnCategory::Unknown },
            country: row.country, last_updated: row.last_updated,
        }).collect())
    }

    pub async fn get_asn_stats(&self) -> Result<(i64, i64, i64), DatabaseError> {
        let hosting = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM asns WHERE category = 'hosting'").fetch_one(&self.pool).await?;
        let residential = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM asns WHERE category = 'residential'").fetch_one(&self.pool).await?;
        let unknown = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM asns WHERE category = 'unknown'").fetch_one(&self.pool).await?;
        Ok((hosting, residential, unknown))
    }

    pub async fn get_asn_count(&self) -> Result<i64, DatabaseError> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM asns").fetch_one(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn get_hosting_ranges(&self) -> Result<Vec<(String, String)>, DatabaseError> {
        sqlx::query_as::<_, (String, String)>("SELECT r.cidr, r.asn FROM asn_ranges r JOIN asns a ON r.asn = a.asn WHERE a.category = 'hosting' ORDER BY a.org").fetch_all(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn get_next_range_to_scan(&self, category: &str) -> Result<Option<AsnRangeRow>, DatabaseError> {
        sqlx::query_as::<_, AsnRangeRow>("SELECT r.* FROM asn_ranges r JOIN asns a ON r.asn = a.asn WHERE a.category = ? ORDER BY r.last_scanned_at ASC, r.scan_offset ASC LIMIT 1").bind(category).fetch_optional(&self.pool).await.map_err(DatabaseError::from)
    }

    pub async fn update_range_progress(&self, cidr: &str, new_offset: i64, reset: bool) -> Result<(), DatabaseError> {
        if reset { sqlx::query("UPDATE asn_ranges SET scan_offset = 0, last_scanned_at = CURRENT_TIMESTAMP WHERE cidr = ?").bind(cidr).execute(&self.pool).await?; }
        else { sqlx::query("UPDATE asn_ranges SET scan_offset = ? WHERE cidr = ?").bind(new_offset).bind(cidr).execute(&self.pool).await?; }
        Ok(())
    }

    pub async fn increment_stats(&self, tier: i32, found_new: bool) -> Result<(), DatabaseError> {
        let date = Utc::now().date_naive().to_string();
        let tier_col = match tier { 1 => "scans_hot", 2 => "scans_warm", _ => "scans_cold" };
        let query = format!("INSERT INTO daily_stats (date, scans_total, {}, discoveries) VALUES (?, 1, 1, ?) ON CONFLICT(date) DO UPDATE SET scans_total = scans_total + 1, {} = {} + 1, discoveries = discoveries + ?", tier_col, tier_col, tier_col);
        sqlx::query(&query).bind(&date).bind(if found_new { 1 } else { 0 }).bind(if found_new { 1 } else { 0 }).execute(&self.pool).await?;
        Ok(())
    }
}
