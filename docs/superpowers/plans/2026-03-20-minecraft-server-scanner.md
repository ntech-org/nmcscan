# Minecraft Server Scanner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a high-performance, safe, and ethical Minecraft Java Edition server scanner with priority-based scanning, strict exclude list enforcement, and Axum HTTP API.

**Architecture:** Single Rust binary with modular components: ExcludeList loader → SQLite database → Custom SLP protocol → Priority Scheduler → Rate-limited scanner → Axum web API. All components run concurrently with tokio.

**Tech Stack:** Rust 2021, tokio (full), sqlx+SQLite, axum 0.7, tower-http, ipnetwork, serde/serde_json, tracing.

---

### Task 1: Project Structure and Dependencies

**Files:**
- Create: `/mnt/storage/projects/NMCScan/Cargo.toml`
- Create: `/mnt/storage/projects/NMCScan/src/main.rs` (empty skeleton)
- Create: `/mnt/storage/projects/NMCScan/assets/index.html`

- [ ] **Step 1: Create Cargo.toml with all dependencies**

```toml
[package]
name = "nmcscan"
version = "0.1.0"
edition = "2021"
description = "High-performance Minecraft Server Scanner with priority-based scanning"
license = "MIT"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite"] }

# Web server
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["compression-gzip", "cors", "fs"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Networking
tokio-util = "0.7"

# IP parsing
ipnetwork = "0.20"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Utilities
thiserror = "1"
chrono = { version = "0.4", features = ["serde"] }
flate2 = "1"

[profile.release]
opt-level = 3
lto = true
strip = true
```

- [ ] **Step 2: Create empty main.rs skeleton**

```rust
//! NMCScan - High-performance Minecraft Server Scanner
//! 
//! A safe, ethical scanner with priority-based scheduling and strict exclude list enforcement.

mod exclude;
mod db;
mod slp;
mod scheduler;
mod api;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting NMCScan...");
    
    // TODO: Initialize components
    // 1. Load exclude list
    // 2. Initialize database
    // 3. Start scheduler/scanner
    // 4. Start web API
    
    Ok(())
}
```

- [ ] **Step 3: Create assets directory and placeholder index.html**

```bash
mkdir -p /mnt/storage/projects/NMCScan/assets
```

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>NMCScan Dashboard</title>
    <style>
        body { font-family: system-ui, sans-serif; margin: 2rem; background: #1a1a2e; color: #eee; }
        h1 { color: #4a9eff; }
        table { width: 100%; border-collapse: collapse; margin-top: 1rem; }
        th, td { padding: 0.75rem; text-align: left; border-bottom: 1px solid #333; }
        th { background: #16213e; color: #4a9eff; }
        tr:hover { background: #1f2940; }
        .online { color: #4ade80; }
        .offline { color: #f87171; }
        .unknown { color: #9ca3af; }
        #stats { display: flex; gap: 2rem; margin-bottom: 2rem; }
        .stat { background: #16213e; padding: 1rem 1.5rem; border-radius: 8px; }
        .stat-value { font-size: 2rem; font-weight: bold; color: #4a9eff; }
        .stat-label { color: #9ca3af; }
    </style>
</head>
<body>
    <h1>🎮 NMCScan Dashboard</h1>
    <div id="stats">
        <div class="stat">
            <div class="stat-value" id="total-servers">-</div>
            <div class="stat-label">Total Servers</div>
        </div>
        <div class="stat">
            <div class="stat-value" id="online-servers">-</div>
            <div class="stat-label">Online</div>
        </div>
        <div class="stat">
            <div class="stat-value" id="total-players">-</div>
            <div class="stat-label">Total Players</div>
        </div>
    </div>
    <h2>Server List</h2>
    <table>
        <thead>
            <tr>
                <th>IP Address</th>
                <th>Status</th>
                <th>Players</th>
                <th>MOTD</th>
                <th>Version</th>
                <th>Last Seen</th>
            </tr>
        </thead>
        <tbody id="server-list"></tbody>
    </table>
    <script>
        async function loadServers() {
            const res = await fetch('/servers?limit=100');
            const servers = await res.json();
            const tbody = document.getElementById('server-list');
            tbody.innerHTML = servers.map(s => `
                <tr>
                    <td>${s.ip}:${s.port}</td>
                    <td class="${s.status}">${s.status}</td>
                    <td>${s.players_online}/${s.players_max}</td>
                    <td>${s.motd || '-'}</td>
                    <td>${s.version || '-'}</td>
                    <td>${s.last_seen ? new Date(s.last_seen).toLocaleString() : '-'}</td>
                </tr>
            `).join('');
            
            const online = servers.filter(s => s.status === 'online');
            document.getElementById('total-servers').textContent = servers.length;
            document.getElementById('online-servers').textContent = online.length;
            document.getElementById('total-players').textContent = 
                online.reduce((sum, s) => sum + (s.players_online || 0), 0);
        }
        loadServers();
        setInterval(loadServers, 30000);
    </script>
</body>
</html>
```

- [ ] **Step 4: Verify project compiles (empty skeleton)**

```bash
cd /mnt/storage/projects/NMCScan && cargo check
```

Expected: Compiles with warnings about unused modules (expected at this stage).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/main.rs assets/index.html
git commit -m "feat: initial project structure with dependencies"
```

---

### Task 2: ExcludeList Module

**Files:**
- Create: `/mnt/storage/projects/NMCScan/src/exclude.rs`

- [ ] **Step 1: Write tests for ExcludeList**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_exclude_cidr() {
        let content = "192.168.0.0/16\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(192, 168, 1, 1).into()));
        assert!(!list.is_excluded(Ipv4Addr::new(10, 0, 0, 1).into()));
    }

    #[test]
    fn test_exclude_single_ip() {
        let content = "153.11.0.1\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(153, 11, 0, 1).into()));
        assert!(!list.is_excluded(Ipv4Addr::new(153, 11, 0, 2).into()));
    }

    #[test]
    fn test_comments_ignored() {
        let content = "# This is a comment\n192.168.1.0/24\n# Another comment\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(192, 168, 1, 100).into()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/storage/projects/NMCScan && cargo test exclude -- --nocapture
```

Expected: FAIL with "unresolved import `exclude::ExcludeList`"

- [ ] **Step 3: Implement ExcludeList struct**

```rust
//! Exclude list module for safe IP filtering.
//! 
//! Parses exclude.conf format (CIDR and single IPs) and provides
//! efficient lookup to avoid scanning protected IP ranges.

use ipnetwork::Ipv4Network;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExcludeListError {
    #[error("Failed to read exclude file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse network: {0}")]
    ParseError(String),
}

/// Holds a list of excluded IP ranges (CIDR) and single IPs.
/// 
/// Before ANY connection attempt, check `if exclude_list.contains(ip)`.
/// If true, SKIP immediately. Do not log, do not ping.
pub struct ExcludeList {
    networks: Vec<Ipv4Network>,
}

impl ExcludeList {
    /// Load exclude list from a file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ExcludeListError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }

    /// Parse exclude list from a string (for testing).
    pub fn from_str(content: &str) -> Result<Self, ExcludeListError> {
        let mut networks = Vec::new();
        
        for line in content.lines() {
            let line = line.trim();
            
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            // Try parsing as CIDR first, then as single IP
            if let Ok(network) = line.parse::<Ipv4Network>() {
                networks.push(network);
            } else if let Ok(ip) = line.parse::<Ipv4Addr>() {
                // Single IP becomes a /32 network
                networks.push(Ipv4Network::new(ip, 32).unwrap());
            } else {
                tracing::warn!("Invalid exclude entry: {}", line);
            }
        }
        
        tracing::info!("Loaded {} exclude networks", networks.len());
        Ok(Self { networks })
    }

    /// Check if an IP address is excluded.
    /// 
    /// # Safety
    /// This method MUST be called before ANY connection attempt.
    /// If true, SKIP immediately. Do not log, do not ping.
    pub fn is_excluded(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => self.networks.iter().any(|n| n.contains(ipv4)),
            IpAddr::V6(_) => false, // We only scan IPv4 for Minecraft
        }
    }

    /// Get the number of excluded networks.
    pub fn len(&self) -> usize {
        self.networks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.networks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exclude_cidr() {
        let content = "192.168.0.0/16\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(192, 168, 1, 1).into()));
        assert!(!list.is_excluded(Ipv4Addr::new(10, 0, 0, 1).into()));
    }

    #[test]
    fn test_exclude_single_ip() {
        let content = "153.11.0.1\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(153, 11, 0, 1).into()));
        assert!(!list.is_excluded(Ipv4Addr::new(153, 11, 0, 2).into()));
    }

    #[test]
    fn test_comments_ignored() {
        let content = "# This is a comment\n192.168.1.0/24\n# Another comment\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(192, 168, 1, 100).into()));
    }

    #[test]
    fn test_military_ranges_excluded() {
        // Test key military ranges from exclude.conf
        let content = "6.0.0.0/8\n7.0.0.0/8\n11.0.0.0/8\n21.0.0.0/8\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(6, 0, 0, 1).into()));
        assert!(list.is_excluded(Ipv4Addr::new(7, 255, 255, 255).into()));
        assert!(list.is_excluded(Ipv4Addr::new(11, 128, 0, 1).into()));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/storage/projects/NMCScan && cargo test exclude -- --nocapture
```

Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add src/exclude.rs
git commit -m "feat: implement ExcludeList with CIDR parsing and safety checks"
```

---

### Task 3: Database Module

**Files:**
- Create: `/mnt/storage/projects/NMCScan/src/db.rs`

- [ ] **Step 1: Write tests for database initialization**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    #[tokio::test]
    async fn test_database_initialization() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db = Database::new(pool).await.unwrap();
        
        // Verify tables exist
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='servers'"
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_server_crud() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db = Database::new(pool).await.unwrap();
        
        // Insert a server
        db.upsert_server(&Server {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            status: "online".to_string(),
            players_online: 10,
            players_max: 20,
            motd: Some("Test Server".to_string()),
            version: Some("1.20.1".to_string()),
            priority: 1,
            last_seen: Some(chrono::Utc::now().naive_utc()),
            consecutive_failures: 0,
            whitelist_prob: 0.0,
        }).await.unwrap();
        
        // Fetch it back
        let server = db.get_server("192.0.2.1").await.unwrap().unwrap();
        assert_eq!(server.players_online, 10);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/storage/projects/NMCScan && cargo test db -- --nocapture
```

Expected: FAIL with "unresolved import `db::Database`"

- [ ] **Step 3: Implement Database module**

```rust
//! Database module with SQLite and WAL mode.
//! 
//! Stores server information with priority-based scheduling support.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::time::Duration;
use thiserror::Error;

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
}

/// Database wrapper with connection pool.
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection with WAL mode enabled.
    pub async fn new(db_path: &str) -> Result<Self, DatabaseError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(30))
            .connect(db_path)
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
                whitelist_prob REAL DEFAULT 0.0
            )
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_priority ON servers(priority)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_status ON servers(status)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_last_seen ON servers(last_seen)")
            .execute(pool)
            .await?;

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
                                priority, last_seen, consecutive_failures, whitelist_prob)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                whitelist_prob = excluded.whitelist_prob
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
    pub async fn get_servers_by_priority(
        &self,
        limit: i32,
    ) -> Result<Vec<Server>, DatabaseError> {
        let servers = sqlx::query_as::<_, Server>(
            "SELECT * FROM servers ORDER BY priority ASC, last_seen ASC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(servers)
    }

    /// Get online servers ordered by player count (for API).
    pub async fn get_online_servers(
        &self,
        limit: i32,
    ) -> Result<Vec<Server>, DatabaseError> {
        let servers = sqlx::query_as::<_, Server>(
            "SELECT * FROM servers WHERE status = 'online' ORDER BY players_online DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(servers)
    }

    /// Get all servers with optional status filter.
    pub async fn get_all_servers(
        &self,
        status_filter: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Server>, DatabaseError> {
        let query = match status_filter {
            Some(status) => format!(
                "SELECT * FROM servers WHERE status = '{}' ORDER BY players_online DESC LIMIT {}",
                status, limit
            ),
            None => format!(
                "SELECT * FROM servers ORDER BY players_online DESC LIMIT {}",
                limit
            ),
        };
        let servers = sqlx::query_as::<_, Server>(&query)
            .fetch_all(&self.pool)
            .await?;
        Ok(servers)
    }

    /// Get server count.
    pub async fn get_server_count(&self) -> Result<i64, DatabaseError> {
        let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM servers")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// Get online server count.
    pub async fn get_online_count(&self) -> Result<i64, DatabaseError> {
        let (count,) = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM servers WHERE status = 'online'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Get total players online.
    pub async fn get_total_players(&self) -> Result<i64, DatabaseError> {
        let (count,) = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE(SUM(players_online), 0) FROM servers WHERE status = 'online'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Update server status to online with priority reset.
    pub async fn mark_online(
        &self,
        ip: &str,
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
    ) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            UPDATE servers SET
                status = 'online',
                players_online = ?,
                players_max = ?,
                motd = ?,
                version = ?,
                priority = 1,
                last_seen = CURRENT_TIMESTAMP,
                consecutive_failures = 0
            WHERE ip = ?
            "#,
        )
        .bind(players_online)
        .bind(players_max)
        .bind(&motd)
        .bind(&version)
        .bind(ip)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update server status to offline with failure increment.
    pub async fn mark_offline(&self, ip: &str) -> Result<(), DatabaseError> {
        sqlx::query(
            r#"
            UPDATE servers SET
                status = 'offline',
                consecutive_failures = consecutive_failures + 1,
                last_seen = CURRENT_TIMESTAMP,
                priority = CASE 
                    WHEN consecutive_failures >= 5 THEN 3
                    ELSE priority
                END
            WHERE ip = ?
            "#,
        )
        .bind(ip)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Insert a new server to scan (if not exists).
    pub async fn insert_server_if_new(&self, ip: &str, port: i32) -> Result<(), DatabaseError> {
        sqlx::query(
            "INSERT OR IGNORE INTO servers (ip, port) VALUES (?, ?)",
        )
        .bind(ip)
        .bind(port)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_initialization() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let db = Database::new("sqlite::memory:").await.unwrap();
        
        // Verify tables exist
        let result: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='servers'"
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(result.0, 1);
    }

    #[tokio::test]
    async fn test_server_crud() {
        let db = Database::new("sqlite::memory:").await.unwrap();
        
        // Insert a server
        db.upsert_server(&Server {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            status: "online".to_string(),
            players_online: 10,
            players_max: 20,
            motd: Some("Test Server".to_string()),
            version: Some("1.20.1".to_string()),
            priority: 1,
            last_seen: Some(Utc::now().naive_utc()),
            consecutive_failures: 0,
            whitelist_prob: 0.0,
        }).await.unwrap();
        
        // Fetch it back
        let server = db.get_server("192.0.2.1").await.unwrap().unwrap();
        assert_eq!(server.players_online, 10);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/storage/projects/NMCScan && cargo test db -- --nocapture
```

Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add src/db.rs
git commit -m "feat: implement SQLite database with WAL mode and server CRUD"
```

---

### Task 4: SLP Protocol Module

**Files:**
- Create: `/mnt/storage/projects/NMCScan/src/slp.rs`

- [ ] **Step 1: Write tests for VarInt encoding**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_encode_decode_small() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 0);
        write_varint(&mut buf, 1);
        write_varint(&mut buf, 127);
        
        let mut cursor = std::io::Cursor::new(&buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 0);
        assert_eq!(read_varint(&mut cursor).unwrap(), 1);
        assert_eq!(read_varint(&mut cursor).unwrap(), 127);
    }

    #[test]
    fn test_varint_encode_decode_large() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 300);
        write_varint(&mut buf, 16383);
        
        let mut cursor = std::io::Cursor::new(&buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 300);
        assert_eq!(read_varint(&mut cursor).unwrap(), 16383);
    }

    #[test]
    fn test_slp_packet_build() {
        let packet = build_handshake("192.0.2.1", 25565, 47);
        assert!(!packet.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/storage/projects/NMCScan && cargo test slp -- --nocapture
```

Expected: FAIL with "unresolved import `slp::*`"

- [ ] **Step 3: Implement SLP module**

```rust
//! Minecraft Server List Ping (SLP) protocol implementation.
//! 
//! Implements the standard handshake and status request packets manually
//! using VarInt encoding. No external mc-ping crates.
//! 
//! Protocol: https://wiki.vg/Server_List_Ping

use serde::Deserialize;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

#[derive(Error, Debug)]
pub enum SlpError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Connection timeout")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("Invalid response format")]
    InvalidResponse,
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Server status response from SLP.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerStatus {
    pub description: Option<Description>,
    pub players: Option<Players>,
    pub version: Option<Version>,
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Description {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Players {
    pub online: i32,
    pub max: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub name: String,
    pub protocol: i32,
}

/// Write a VarInt to the buffer.
/// Minecraft uses variable-length integers for packet lengths and IDs.
pub fn write_varint(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut part = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            part |= 0x80;
        }
        buf.push(part);
        if value == 0 {
            break;
        }
    }
}

/// Read a VarInt from a reader.
pub fn read_varint<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut result = 0u32;
    let mut shift = 0;

    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let byte = byte[0];

        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            break;
        }

        shift += 7;
        if shift >= 35 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "VarInt too long",
            ));
        }
    }

    Ok(result)
}

/// Build a Handshake packet.
/// Packet ID: 0x00
pub fn build_handshake(host: &str, port: u16, protocol_version: i32) -> Vec<u8> {
    let mut packet = Vec::new();

    // Packet ID (VarInt)
    write_varint(&mut packet, 0);

    // Protocol Version (VarInt)
    write_varint(&mut packet, protocol_version as u32);

    // Server Address (String = VarInt length + UTF-8 bytes)
    let host_bytes = host.as_bytes();
    write_varint(&mut packet, host_bytes.len() as u32);
    packet.extend_from_slice(host_bytes);

    // Server Port (Unsigned Short, big-endian)
    packet.extend_from_slice(&port.to_be_bytes());

    // Next State (VarInt, 1 = Status)
    write_varint(&mut packet, 1);

    // Wrap in length-prefixed packet
    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);

    final_packet
}

/// Build a Status Request packet.
/// Packet ID: 0x00 (for status state)
pub fn build_status_request() -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, 0); // Packet ID

    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);

    final_packet
}

/// Perform a Server List Ping and return server status.
/// 
/// # Safety
/// - Always check exclude list BEFORE calling this function
/// - Timeout is hardcoded to 3 seconds
/// - Does NOT attempt login or authentication
pub async fn ping_server(addr: SocketAddr) -> Result<ServerStatus, SlpError> {
    // Connect with timeout
    let mut stream = timeout(Duration::from_secs(3), TcpStream::connect(addr)).await??;
    stream.set_nodelay(true)?;

    // Send Handshake packet
    let handshake = build_handshake(&addr.ip().to_string(), addr.port(), 47);
    stream.write_all(&handshake).await?;

    // Send Status Request packet
    let status_request = build_status_request();
    stream.write_all(&status_request).await?;

    // Read response length (VarInt)
    let mut len_buf = [0u8; 5]; // Max VarInt is 5 bytes
    let mut bytes_read = 0;
    
    loop {
        let n = timeout(Duration::from_secs(3), stream.read(&mut len_buf[bytes_read..bytes_read+1])).await??;
        if n == 0 {
            return Err(SlpError::InvalidResponse);
        }
        bytes_read += 1;
        
        // Check if this is the last byte of VarInt
        if len_buf[bytes_read - 1] & 0x80 == 0 {
            break;
        }
        if bytes_read >= 5 {
            return Err(SlpError::InvalidResponse);
        }
    }

    let mut cursor = io::Cursor::new(&len_buf[..bytes_read]);
    let response_len = read_varint(&mut cursor)? as usize;

    // Read response JSON
    let mut response_buf = vec![0u8; response_len];
    timeout(Duration::from_secs(3), stream.read_exact(&mut response_buf)).await??;

    // Parse JSON response
    let response: ServerStatus = serde_json::from_slice(&response_buf)?;
    Ok(response)
}

/// Extract MOTD text from description.
pub fn extract_motd(status: &ServerStatus) -> String {
    status
        .description
        .as_ref()
        .map(|d| d.text.clone())
        .unwrap_or_else(|| "Unknown Server".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_varint_encode_decode_small() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 0);
        write_varint(&mut buf, 1);
        write_varint(&mut buf, 127);
        
        let mut cursor = Cursor::new(&buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 0);
        assert_eq!(read_varint(&mut cursor).unwrap(), 1);
        assert_eq!(read_varint(&mut cursor).unwrap(), 127);
    }

    #[test]
    fn test_varint_encode_decode_large() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 300);
        write_varint(&mut buf, 16383);
        
        let mut cursor = Cursor::new(&buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 300);
        assert_eq!(read_varint(&mut cursor).unwrap(), 16383);
    }

    #[test]
    fn test_slp_packet_build() {
        let packet = build_handshake("192.0.2.1", 25565, 47);
        assert!(!packet.is_empty());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/storage/projects/NMCScan && cargo test slp -- --nocapture
```

Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add src/slp.rs
git commit -m "feat: implement custom SLP protocol with VarInt encoding"
```

---

### Task 5: Priority Scheduler Module

**Files:**
- Create: `/mnt/storage/projects/NMCScan/src/scheduler.rs`

- [ ] **Step 1: Write tests for priority logic**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_adjustment_online() {
        let mut server = ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 3,
            consecutive_failures: 5,
        };
        
        server.mark_online();
        assert_eq!(server.priority, 1);
        assert_eq!(server.consecutive_failures, 0);
    }

    #[test]
    fn test_priority_adjustment_offline() {
        let mut server = ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 2,
            consecutive_failures: 4,
        };
        
        server.mark_offline();
        assert_eq!(server.consecutive_failures, 5);
        assert_eq!(server.priority, 3); // Should become cold after 5 failures
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/storage/projects/NMCScan && cargo test scheduler -- --nocapture
```

Expected: FAIL

- [ ] **Step 3: Implement Scheduler module**

```rust
//! Priority-based scheduler for efficient server scanning.
//! 
//! Implements Hot/Warm/Cold tier algorithm:
//! - Tier 1 (Hot): Online servers, last seen < 4 hours
//! - Tier 2 (Warm): Known hosting ASN ranges, not scanned in 7 days
//! - Tier 3 (Cold): High-failure servers, very slow scan

use chrono::{Duration, Utc};
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Server target for scanning.
#[derive(Debug, Clone)]
pub struct ServerTarget {
    pub ip: String,
    pub port: u16,
    pub priority: i32,
    pub consecutive_failures: i32,
}

impl ServerTarget {
    pub fn new(ip: String, port: u16) -> Self {
        Self {
            ip,
            port,
            priority: 2, // Default to Warm
            consecutive_failures: 0,
        }
    }

    /// Mark server as online: reset failures, set priority to Hot.
    pub fn mark_online(&mut self) {
        self.consecutive_failures = 0;
        self.priority = 1; // Hot
    }

    /// Mark server as offline: increment failures, potentially demote to Cold.
    pub fn mark_offline(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures > 5 {
            self.priority = 3; // Cold
        }
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip.parse().unwrap(), self.port)
    }
}

/// Simulated hosting ASN ranges for Warm tier.
/// In production, this would be loaded from a GeoIP/ASN database.
pub const HOSTING_ASN_RANGES: &[&str] = &[
    "5.9.0.0/16",     // Hetzner
    "46.4.0.0/16",    // Hetzner
    "78.46.0.0/15",   // Hetzner
    "88.198.0.0/16",  // Hetzner
    "116.202.0.0/16", // Hetzner
    "135.181.0.0/16", // Hetzner
    "138.201.0.0/16", // Hetzner
    "142.132.0.0/16", // Hetzner
    "144.76.0.0/16",  // Hetzner
    "148.251.0.0/16", // Hetzner
    "157.90.0.0/16",  // Hetzner
    "159.69.0.0/16",  // Hetzner
    "162.55.0.0/16",  // Hetzner
    "167.233.0.0/16", // Hetzner
    "168.119.0.0/16", // Hetzner
    "176.9.0.0/16",   // Hetzner
    "188.40.0.0/16",  // Hetzner
    "195.201.0.0/16", // Hetzner
    "213.133.96.0/19",// Hetzner
    "104.16.0.0/12",  // Cloudflare
    "172.64.0.0/13",  // Cloudflare
    "35.192.0.0/12",  // GCP
    "34.64.0.0/10",   // GCP
    "52.0.0.0/6",     // AWS
    "54.0.0.0/8",     // AWS
    "13.32.0.0/15",   // AWS
    "20.0.0.0/4",     // Azure
    "40.64.0.0/10",   // Azure
];

/// Priority scheduler managing Hot/Warm/Cold queues.
pub struct Scheduler {
    hot_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    warm_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    cold_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            hot_queue: Arc::new(Mutex::new(VecDeque::new())),
            warm_queue: Arc::new(Mutex::new(VecDeque::new())),
            cold_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Add a server to the appropriate queue based on priority.
    pub async fn add_server(&self, server: ServerTarget) {
        let queue = match server.priority {
            1 => &self.hot_queue,
            2 => &self.warm_queue,
            _ => &self.cold_queue,
        };
        queue.lock().await.push_back(server);
    }

    /// Get the next server to scan (priority order: Hot > Warm > Cold).
    pub async fn next_server(&self) -> Option<ServerTarget> {
        // Try Hot queue first
        if let Some(server) = self.hot_queue.lock().await.pop_front() {
            return Some(server);
        }

        // Then Warm queue
        if let Some(server) = self.warm_queue.lock().await.pop_front() {
            return Some(server);
        }

        // Finally Cold queue (very limited)
        if let Some(server) = self.cold_queue.lock().await.pop_front() {
            return Some(server);
        }

        None
    }

    /// Re-queue a server after scanning with updated status.
    pub async fn requeue_server(&self, mut server: ServerTarget, was_online: bool) {
        if was_online {
            server.mark_online();
        } else {
            server.mark_offline();
        }
        self.add_server(server).await;
    }

    /// Get queue sizes for monitoring.
    pub async fn get_queue_sizes(&self) -> (usize, usize, usize) {
        (
            self.hot_queue.lock().await.len(),
            self.warm_queue.lock().await.len(),
            self.cold_queue.lock().await.len(),
        )
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_priority_adjustment_online() {
        let mut server = ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 3,
            consecutive_failures: 5,
        };
        
        server.mark_online();
        assert_eq!(server.priority, 1);
        assert_eq!(server.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_priority_adjustment_offline() {
        let mut server = ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 2,
            consecutive_failures: 4,
        };
        
        server.mark_offline();
        assert_eq!(server.consecutive_failures, 5);
        assert_eq!(server.priority, 3);
    }

    #[tokio::test]
    async fn test_scheduler_priority_order() {
        let scheduler = Scheduler::new();
        
        // Add servers to different queues
        scheduler.add_server(ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 3,
            consecutive_failures: 0,
        }).await;
        
        scheduler.add_server(ServerTarget {
            ip: "192.0.2.2".to_string(),
            port: 25565,
            priority: 1,
            consecutive_failures: 0,
        }).await;
        
        scheduler.add_server(ServerTarget {
            ip: "192.0.2.3".to_string(),
            port: 25565,
            priority: 2,
            consecutive_failures: 0,
        }).await;
        
        // Should return Hot first
        let next = scheduler.next_server().await.unwrap();
        assert_eq!(next.ip, "192.0.2.2");
        assert_eq!(next.priority, 1);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/storage/projects/NMCScan && cargo test scheduler -- --nocapture
```

Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add src/scheduler.rs
git commit -m "feat: implement priority scheduler with Hot/Warm/Cold tiers"
```

---

### Task 6: Scanner with Rate Limiting

**Files:**
- Create: `/mnt/storage/projects/NMCScan/src/scanner.rs`

- [ ] **Step 1: Write integration test for scanner**

```rust
// Integration test - will be skipped in CI without real servers
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_scan_real_server() {
        let exclude_list = ExcludeList::from_str("").unwrap();
        let db = Database::new("sqlite::memory:").await.unwrap();
        
        let result = scan_server(
            "hypixel.net".parse().unwrap(),
            &exclude_list,
            &db,
        ).await;
        
        // Hypixel should be online
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Implement Scanner module**

```rust
//! Rate-limited concurrent scanner.
//! 
//! Hardcoded limits:
//! - Max 200 simultaneous tasks
//! - ~100 new connections per second
//! - 3 second timeout per connection

use crate::db::Database;
use crate::exclude::ExcludeList;
use crate::slp::{ping_server, extract_motd, SlpError};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Semaphore, Mutex};
use tokio::time;
use tracing;

/// Maximum concurrent scan tasks.
const MAX_CONCURRENCY: usize = 200;

/// Connections per second limit.
const RATE_LIMIT_PER_SEC: u64 = 100;

/// Scanner with rate limiting and concurrency control.
pub struct Scanner {
    semaphore: Arc<Semaphore>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    exclude_list: Arc<ExcludeList>,
    db: Arc<Database>,
}

struct RateLimiter {
    tokens: u64,
    last_refill: time::Instant,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            tokens: RATE_LIMIT_PER_SEC,
            last_refill: time::Instant::now(),
        }
    }

    async fn acquire(&mut self) {
        loop {
            let now = time::Instant::now();
            let elapsed = now.duration_since(self.last_refill).as_secs();
            
            if elapsed >= 1 {
                self.tokens = RATE_LIMIT_PER_SEC;
                self.last_refill = now;
            }
            
            if self.tokens > 0 {
                self.tokens -= 1;
                break;
            }
            
            // Wait a bit before retrying
            time::sleep(Duration::from_millis(10)).await;
        }
    }
}

impl Scanner {
    pub fn new(exclude_list: ExcludeList, db: Database) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENCY)),
            rate_limiter: Arc::new(Mutex::new(RateLimiter::new())),
            exclude_list: Arc::new(exclude_list),
            db: Arc::new(db),
        }
    }

    /// Scan a single server with safety checks.
    /// 
    /// # Safety
    /// - Checks exclude list BEFORE any connection
    /// - If excluded, SKIP immediately (no log, no ping)
    pub async fn scan_server(&self, ip: &str, port: u16) -> Result<bool, SlpError> {
        // Parse IP
        let ip_addr: IpAddr = match ip.parse() {
            Ok(addr) => addr,
            Err(_) => return Err(SlpError::InvalidResponse),
        };

        // CRITICAL SAFETY CHECK: Exclude list enforcement
        // If true, SKIP immediately. Do not log, do not ping.
        if self.exclude_list.is_excluded(ip_addr) {
            return Err(SlpError::InvalidResponse);
        }

        // Acquire rate limit token
        let mut rate_limiter = self.rate_limiter.lock().await;
        rate_limiter.acquire().await;
        drop(rate_limiter);

        // Acquire concurrency permit
        let _permit = self.semaphore.acquire().await.unwrap();

        // Perform the ping
        let addr = SocketAddr::new(ip_addr, port);
        match ping_server(addr).await {
            Ok(status) => {
                // Server is online
                let players_online = status.players.as_ref().map(|p| p.online).unwrap_or(0);
                let players_max = status.players.as_ref().map(|p| p.max).unwrap_or(0);
                let motd = status.description.as_ref().map(|d| d.text.clone());
                let version = status.version.as_ref().map(|v| v.name.clone());

                self.db.mark_online(ip, players_online, players_max, motd, version).await.unwrap();
                tracing::debug!("Server {}:{} is online ({} players)", ip, port, players_online);
                Ok(true)
            }
            Err(_) => {
                // Server is offline or unreachable
                self.db.mark_offline(ip).await.unwrap();
                Ok(false)
            }
        }
    }

    /// Scan multiple servers concurrently with rate limiting.
    pub async fn scan_batch(&self, servers: Vec<(String, u16)>) -> Vec<(String, bool)> {
        let tasks: Vec<_> = servers
            .into_iter()
            .map(|(ip, port)| {
                let scanner = Arc::clone(&self);
                tokio::spawn(async move {
                    let result = scanner.scan_server(&ip, port).await;
                    (ip, result.unwrap_or(false))
                })
            })
            .collect();

        let mut results = Vec::new();
        for task in tasks {
            if let Ok(result) = task.await {
                results.push(result);
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exclude::ExcludeList;

    #[tokio::test]
    async fn test_excluded_server_skipped() {
        let exclude_list = ExcludeList::from_str("192.0.2.1\n").unwrap();
        let db = Database::new("sqlite::memory:").await.unwrap();
        let scanner = Scanner::new(exclude_list, db);

        // Should fail immediately without attempting connection
        let result = scanner.scan_server("192.0.2.1", 25565).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

```bash
cd /mnt/storage/projects/NMCScan && cargo test scanner -- --nocapture
```

Expected: PASS (1 test)

- [ ] **Step 4: Commit**

```bash
git add src/scanner.rs
git commit -m "feat: implement rate-limited scanner with 200 max concurrency"
```

---

### Task 7: Axum Web API

**Files:**
- Create: `/mnt/storage/projects/NMCScan/src/api.rs`

- [ ] **Step 1: Implement API module**

```rust
//! Axum web API for server monitoring.
//! 
//! Endpoints:
//! - GET /health - Health check with server count
//! - GET /servers?limit=50&status=online - List servers
//! - GET /server/{ip} - Server details
//! - GET / - Static HTML dashboard

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

use crate::db::{Database, Server};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
}

/// Query parameters for /servers endpoint.
#[derive(Deserialize)]
pub struct ServerQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub status: Option<String>,
}

fn default_limit() -> i32 {
    50
}

/// Health check response.
#[derive(serde::Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub total_servers: i64,
}

/// Create the Axum router with all endpoints.
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health_check))
        .route("/servers", get(list_servers))
        .route("/server/:ip", get(get_server))
        .route("/", get(dashboard))
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(state)
}

/// GET /health - Health check endpoint.
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let total_servers = state.db.get_server_count().await.unwrap_or(0);
    Json(HealthResponse {
        status: "ok".to_string(),
        total_servers,
    })
}

/// GET /servers - List servers with optional filters.
async fn list_servers(
    State(state): State<AppState>,
    Query(query): Query<ServerQuery>,
) -> Json<Vec<Server>> {
    let servers = state
        .db
        .get_all_servers(query.status.as_deref(), query.limit)
        .await
        .unwrap_or_default();
    Json(servers)
}

/// GET /server/{ip} - Get server details.
async fn get_server(
    State(state): State<AppState>,
    Path(ip): Path<String>,
) -> Result<Json<Server>, StatusCode> {
    state
        .db
        .get_server(&ip)
        .await
        .unwrap()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET / - Static HTML dashboard.
async fn dashboard() -> Html<&'static str> {
    Html(include_str!("../assets/index.html"))
}

/// Start the web server.
pub async fn run_server(state: AppState, addr: &str) -> std::io::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Web API listening on {}", addr);
    axum::serve(listener, app).await
}
```

- [ ] **Step 2: Commit**

```bash
git add src/api.rs
git commit -m "feat: implement Axum web API with /health, /servers, /server/{ip}, /"
```

---

### Task 8: Main Integration

**Files:**
- Modify: `/mnt/storage/projects/NMCScan/src/main.rs`

- [ ] **Step 1: Update main.rs with full integration**

```rust
//! NMCScan - High-performance Minecraft Server Scanner
//! 
//! A safe, ethical scanner with priority-based scheduling and strict exclude list enforcement.
//! 
//! # Safety Features
//! - Strict exclude.conf enforcement (US Military, Universities, complaining IPs)
//! - Rate limiting (~100 connections/sec)
//! - Concurrency limiting (max 200 simultaneous tasks)
//! - 3-second timeout per connection
//! - No authentication attempts, no exploit scanning

mod api;
mod db;
mod exclude;
mod scheduler;
mod scanner;
mod slp;

use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("🎮 Starting NMCScan...");

    // 1. Load exclude list
    tracing::info!("Loading exclude list...");
    let exclude_list = exclude::ExcludeList::from_file("exclude.conf")
        .unwrap_or_else(|e| {
            tracing::warn!("Could not load exclude.conf: {}", e);
            tracing::warn!("Using empty exclude list - BE CAREFUL!");
            exclude::ExcludeList::from_str("").unwrap()
        });
    tracing::info!("Loaded {} exclude networks", exclude_list.len());

    // 2. Initialize database
    tracing::info!("Initializing database...");
    let db = db::Database::new("sqlite:nmcscan.db").await?;
    let db = Arc::new(db);

    // 3. Create scanner and scheduler
    let scanner = scanner::Scanner::new(
        exclude::ExcludeList::from_file("exclude.conf").unwrap_or_else(|_| {
            exclude::ExcludeList::from_str("").unwrap()
        }),
        (*db).clone(),
    );
    let scheduler = scheduler::Scheduler::new();

    // 4. Start background scanner task
    let scanner_handle = {
        let scheduler = Arc::new(scheduler);
        let scanner = Arc::new(scanner);
        tokio::spawn(async move {
            run_scanner_loop(scanner, scheduler).await;
        })
    };

    // 5. Start web API
    let api_state = api::AppState { db };
    let api_handle = tokio::spawn(async move {
        api::run_server(api_state, "0.0.0.0:3000").await.unwrap();
    });

    // Wait for both tasks
    tokio::select! {
        _ = scanner_handle => tracing::info!("Scanner stopped"),
        _ = api_handle => tracing::info!("API stopped"),
    }

    Ok(())
}

/// Background scanner loop.
async fn run_scanner_loop(
    scanner: Arc<scanner::Scanner>,
    scheduler: Arc<scheduler::Scheduler>,
) {
    tracing::info!("Scanner loop started");

    loop {
        // Get next server from scheduler
        if let Some(server) = scheduler.next_server().await {
            tracing::debug!("Scanning {}:{}", server.ip, server.port);
            
            let was_online = scanner
                .scan_server(&server.ip, server.port)
                .await
                .unwrap_or(false);

            // Re-queue with updated priority
            scheduler.requeue_server(server, was_online).await;
        } else {
            // No servers to scan, wait a bit
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
}
```

- [ ] **Step 2: Copy exclude.conf to project root**

```bash
cp /mnt/storage/projects/NMCScan/scanner-old/exclude.conf /mnt/storage/projects/NMCScan/exclude.conf
```

- [ ] **Step 3: Build and verify**

```bash
cd /mnt/storage/projects/NMCScan && cargo build --release
```

Expected: Compiles successfully with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs exclude.conf
git commit -m "feat: integrate all components in main.rs"
```

---

### Task 9: Documentation and Testing

**Files:**
- Create: `/mnt/storage/projects/NMCScan/README.md`

- [ ] **Step 1: Create comprehensive README**

```markdown
# NMCScan - Minecraft Server Scanner

High-performance, safe, and ethical Minecraft Java Edition server scanner with priority-based scheduling.

## 🛡️ Safety Features

- **Strict Exclude List**: Parses `exclude.conf` to avoid US Military ranges, Universities, and complaining IPs
- **Rate Limiting**: ~100 connections/second maximum
- **Concurrency Limiting**: Max 200 simultaneous tasks
- **Timeout**: 3 seconds per connection
- **Ethical**: Uses only standard Server List Ping (SLP), no authentication attempts

## 🏗️ Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ ExcludeList │────▶│   Scanner    │────▶│   SQLite    │
└─────────────┘     └──────────────┘     └─────────────┘
                           │
                           ▼
                    ┌──────────────┐     ┌─────────────┐
                    │  Scheduler   │────▶│   Axum API  │
                    │ (Hot/Warm/   │     │  (Port 3000)│
                    │   Cold)      │     └─────────────┘
                    └──────────────┘
```

## 🚀 Quick Start

### Build

```bash
cargo build --release
```

### Run

```bash
# Copy exclude.conf if not present
cp scanner-old/exclude.conf .

# Run the scanner
./target/release/nmcscan
```

### Configuration

- **Database**: `nmcscan.db` (SQLite, auto-created)
- **Exclude List**: `exclude.conf` (required for safety)
- **Web API**: `http://0.0.0.0:3000`
- **Log Level**: Set `RUST_LOG=debug` for verbose output

## 📡 API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | HTML dashboard |
| `GET /health` | Health check: `{"status": "ok", "total_servers": 123}` |
| `GET /servers?limit=50&status=online` | List servers (ordered by players) |
| `GET /server/{ip}` | Server details |

## 🧠 Priority Algorithm

- **Tier 1 (Hot)**: Online servers, last seen < 4 hours
- **Tier 2 (Warm)**: Known hosting ASN ranges, not scanned in 7 days
- **Tier 3 (Cold)**: High-failure servers (>5 failures), very slow scan

## 📊 Database Schema

```sql
CREATE TABLE servers (
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
    whitelist_prob REAL DEFAULT 0.0
);
```

## ⚙️ Performance Tuning

For low-resource VPS (1 vCPU, 1GB RAM):

- Already optimized with `lto = true` and `strip = true`
- SQLite WAL mode for concurrent reads
- Rate limiting prevents network saturation

## 📝 License

MIT License
```

- [ ] **Step 2: Final verification**

```bash
cd /mnt/storage/projects/NMCScan && cargo test && cargo build --release
```

Expected: All tests pass, release build succeeds.

- [ ] **Step 3: Final commit**

```bash
git add README.md
git commit -m "docs: add comprehensive README with usage instructions"
```

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-03-20-minecraft-server-scanner.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
