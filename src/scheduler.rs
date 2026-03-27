//! Priority-based scheduler for efficient server scanning.
//!
//! Implements Hot/Warm/Cold tier algorithm with ethical weighted selection.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
// use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use crate::db::Database;
use crate::asn::AsnCategory;
use rand::prelude::*;
use rand::seq::SliceRandom;
use sqlx::Row;

/// Target server for scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerTarget {
    pub ip: String,
    pub port: u16,
    pub hostname: Option<String>,
    pub priority: i32,
    pub category: AsnCategory,
    pub last_scanned: Option<DateTime<Utc>>,
    pub next_scan_at: Option<DateTime<Utc>>,
    pub consecutive_failures: i32,
    pub server_type: String,
    #[serde(default)]
    pub is_discovery: bool,
    pub scan_count: i32,
    pub success_rate: f32,
    pub direction: i8,
}

impl ServerTarget {
    pub fn new(ip: String, port: u16, server_type: String) -> Self {
        Self {
            ip,
            port,
            hostname: None,
            priority: 2, // Warm by default
            category: AsnCategory::Unknown,
            last_scanned: None,
            next_scan_at: None,
            consecutive_failures: 0,
            scan_count: 0,
            success_rate: 0.0,
            server_type,
            direction: 0,
            is_discovery: false,
        }
    }

    pub fn mark_online(&mut self) {
        self.priority = 1; // Move to Hot
        self.consecutive_failures = 0;
        self.scan_count += 1;
        self.success_rate = (self.success_rate * (self.scan_count - 1) as f32 + 1.0) / self.scan_count as f32;
    }

    pub fn mark_offline(&mut self) {
        self.consecutive_failures += 1;
        self.scan_count += 1;
        self.success_rate = (self.success_rate * (self.scan_count - 1) as f32) / self.scan_count as f32;
        
        if self.consecutive_failures > 5 {
            self.priority = 3; // Move to Cold
        }
    }
}

pub struct Scheduler {
    hot_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    warm_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    cold_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    pub db: Arc<Database>,
    pub test_mode: bool,
    test_interval: u32,
    asn_scan_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    asn_last_scanned: Arc<Mutex<std::collections::HashMap<String, DateTime<Utc>>>>,
    asn_ranges_cache: Arc<Mutex<Vec<crate::db::AsnRangeRow>>>,
}

impl Scheduler {
    pub fn new(db: Arc<Database>, test_mode: bool, test_interval: u32) -> Self {
        Self {
            hot_queue: Arc::new(Mutex::new(VecDeque::new())),
            warm_queue: Arc::new(Mutex::new(VecDeque::new())),
            cold_queue: Arc::new(Mutex::new(VecDeque::new())),
            db,
            test_mode,
            test_interval,
            asn_scan_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
            asn_last_scanned: Arc::new(Mutex::new(std::collections::HashMap::new())),
            asn_ranges_cache: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_server(&self, server: ServerTarget, at_front: bool) {
        let queue = match server.priority {
            1 => &self.hot_queue,
            2 => &self.warm_queue,
            _ => &self.cold_queue,
        };
        let mut q = queue.lock().await;
        if at_front {
            q.push_front(server);
        } else {
            q.push_back(server);
        }
    }

    /// Get the next server to scan using weighted random selection to prevent tier starvation.
    pub async fn next_server(&self) -> Option<ServerTarget> {
        let now = Utc::now();
        let roll = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..100)
        };

        // Define preferred order based on roll
        let tiers = if roll < 70 {
            vec![1, 2, 3] // 70% Hot
        } else if roll < 90 {
            vec![2, 1, 3] // 20% Warm
        } else {
            vec![3, 1, 2] // 10% Cold
        };

        for tier in tiers {
            let queue = match tier {
                1 => &self.hot_queue,
                2 => &self.warm_queue,
                _ => &self.cold_queue,
            };

            let mut q = queue.lock().await;
            if q.is_empty() { continue; }
            
            // Check up to 5000 elements to find one that is ready to scan.
            let search_limit = std::cmp::min(q.len(), 5000);
            for i in 0..search_limit {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    return q.remove(i);
                }
            }
        }
        None
    }

    pub async fn fill_warm_queue_if_needed(&self) {
        let warm_len = self.warm_queue.lock().await.len();
        if warm_len > 10000 { return; } 

        // INTERLEAVED DISCOVERY: Fetch up to 500 hosting ranges
        if let Ok(ranges) = self.db.get_ranges_to_scan("hosting", 500).await {
            if ranges.is_empty() { return; }
            tracing::info!("Discovery: Filling Warm queue by interleaving {} hosting ranges", ranges.len());
            let _ = self.fill_discovery_queue(ranges, 2, 50).await;
        }
    }

    pub async fn fill_cold_queue_if_needed(&self) {
        let cold_len = self.cold_queue.lock().await.len();
        if cold_len > 5000 { return; }

        // 1. Try to recycle dead/ignored servers
        if let Ok(dead_servers) = sqlx::query_as::<_, crate::db::Server>(
            "SELECT * FROM servers WHERE priority = 3 ORDER BY last_seen ASC LIMIT 1000",
        )
        .fetch_all(self.db.pool())
        .await {
            for server in dead_servers {
                let mut target = ServerTarget::new(server.ip, server.port as u16, server.server_type);
                target.priority = 3;
                if let Some(last) = server.last_seen {
                    target.next_scan_at = Some(last.and_utc() + chrono::Duration::days(7));
                }
                self.add_server(target, false).await;
            }
        }

        // 2. INTERLEAVED DISCOVERY: Fetch up to 200 residential/unknown ranges
        let mut ranges = self.db.get_ranges_to_scan("residential", 200).await.unwrap_or_default();
        if ranges.is_empty() {
            ranges = self.db.get_ranges_to_scan("unknown", 200).await.unwrap_or_default();
        }

        if !ranges.is_empty() {
            tracing::info!("Discovery: Filling Cold queue by interleaving {} ranges", ranges.len());
            let _ = self.fill_discovery_queue(ranges, 3, 50).await;
        }
    }

    /// Master discovery function that takes multiple ranges, pulls a few IPs from each,
    /// shuffles the aggregate list, and pushes to the queue.
    pub async fn fill_discovery_queue(
        &self,
        ranges: Vec<crate::db::AsnRangeRow>,
        priority: i32,
        ips_per_range: usize,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use ipnetwork::Ipv4Network;
        use std::collections::HashSet;
        
        let mut all_candidates = Vec::new();
        let mut updates = Vec::new();

        for range in ranges {
            let network: Ipv4Network = match range.cidr.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let total_ips = network.size() as i64;
            let mut current_offset = range.scan_offset;
            let mut added = 0;

            while added < ips_per_range && current_offset < total_ips {
                if let Some(ip) = network.nth(current_offset as u32) {
                    all_candidates.push(ip.to_string());
                    added += 1;
                }
                current_offset += 1;
            }

            if added > 0 {
                let is_done = current_offset >= total_ips;
                updates.push((range.cidr.clone(), current_offset, is_done));
                self.record_asn_scan(&range.asn).await;
            }
        }

        if all_candidates.is_empty() { return Ok(()); }

        // BATCH CHECK: Find which IPs are already in our database in one go
        let mut known_ips = HashSet::new();
        // SQLite has a limit on variables, so we check in chunks of 500
        for chunk in all_candidates.chunks(500) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!("SELECT DISTINCT ip FROM servers WHERE ip IN ({})", placeholders);
            
            let mut sql_query = sqlx::query(&query);
            for ip in chunk {
                sql_query = sql_query.bind(ip);
            }

            if let Ok(rows) = sql_query.fetch_all(self.db.pool()).await {
                for row in rows {
                    if let Ok(ip) = row.try_get::<String, _>(0) {
                        known_ips.insert(ip);
                    }
                }
            }
        }

        let mut all_targets = Vec::new();
        for ip_str in all_candidates {
            if !known_ips.contains(&ip_str) {
                let mut java_target = ServerTarget::new(ip_str.clone(), 25565, "java".to_string());
                java_target.category = if priority == 2 { AsnCategory::Hosting } else { AsnCategory::Residential };
                java_target.priority = priority;
                java_target.is_discovery = true;
                all_targets.push(java_target);
                
                let mut bedrock_target = ServerTarget::new(ip_str, 19132, "bedrock".to_string());
                bedrock_target.category = if priority == 2 { AsnCategory::Hosting } else { AsnCategory::Residential };
                bedrock_target.priority = priority;
                bedrock_target.is_discovery = true;
                all_targets.push(bedrock_target);
            }
        }

        // Batch update progress in DB
        self.db.update_batch_range_progress(updates).await?;

        // GLOBAL SHUFFLE & ADD
        if !all_targets.is_empty() {
            {
                let mut rng = rand::thread_rng();
                all_targets.shuffle(&mut rng);
            }
            
            for target in all_targets {
                self.add_server(target, true).await;
            }
        }

        Ok(())
    }

    /// Synchronous helper to sample IPs to avoid holding non-Send ThreadRng across awaits.
    fn sample_ips_from_network(&self, network: &ipnetwork::Ipv4Network, max_to_add: usize) -> Vec<std::net::Ipv4Addr> {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let mut sampled_ips = Vec::new();
        let total_ips = network.size() as u64;

        if total_ips <= 500 {
            let mut all_ips: Vec<_> = network.iter().collect();
            all_ips.shuffle(&mut rng);
            sampled_ips = all_ips.into_iter().take(max_to_add).collect();
        } else {
            let mut picked_indices = std::collections::HashSet::new();
            let mut attempts = 0;
            while sampled_ips.len() < max_to_add && attempts < max_to_add * 3 {
                let idx = rng.gen_range(0..total_ips);
                if picked_indices.insert(idx) {
                    if let Some(ip) = network.nth(idx as u32) {
                        sampled_ips.push(ip);
                    }
                }
                attempts += 1;
            }
        }
        sampled_ips
    }

    pub async fn requeue_server(&self, mut server: ServerTarget, was_online: bool) {
        let is_new_discovery = server.last_scanned.is_none();
        let now = Utc::now();
        server.last_scanned = Some(now);

        if was_online {
            server.mark_online();
            
            // Progressive Port Scanning Logic (only for Java servers)
            // User requested: "make sure the progressive scanning is in the hot stage instead of the discovery stage"
            // This means we only do it if the server was already in our DB (!is_new_discovery)
            if server.server_type == "java" && !is_new_discovery {
                if server.direction == 0 {
                    // Start progressive scan from default port
                    if server.port == 25565 {
                        let mut t_up = ServerTarget::new(server.ip.clone(), server.port + 1, "java".to_string());
                        t_up.direction = 1;
                        t_up.category = server.category.clone();
                        self.add_server(t_up, false).await;
                        
                        let mut t_down = ServerTarget::new(server.ip.clone(), server.port - 1, "java".to_string());
                        t_down.direction = -1;
                        t_down.category = server.category.clone();
                        self.add_server(t_down, false).await;
                    }
                } else if server.direction == 1 && server.port < 65535 {
                    // Continue scanning upwards
                    let mut t_up = ServerTarget::new(server.ip.clone(), server.port + 1, "java".to_string());
                    t_up.direction = 1;
                    t_up.category = server.category.clone();
                    self.add_server(t_up, false).await;
                } else if server.direction == -1 && server.port > 1 {
                    // Continue scanning downwards
                    let mut t_down = ServerTarget::new(server.ip.clone(), server.port - 1, "java".to_string());
                    t_down.direction = -1;
                    t_down.category = server.category.clone();
                    self.add_server(t_down, false).await;
                }
            }
        } else {
            server.mark_offline();
        }

        // If it's a new discovery target and it's offline, don't re-queue it.
        // This prevents the memory queue from being filled with thousands of offline IPs.
        if is_new_discovery && !was_online {
            tracing::debug!("Dropping offline discovery target: {}:{}", server.ip, server.port);
            return;
        }

        let delay = if self.test_mode {
            chrono::Duration::seconds(self.test_interval as i64)
        } else {
            match server.priority {
                1 => chrono::Duration::hours(4),
                2 => chrono::Duration::hours(24),
                _ => chrono::Duration::days(7),
            }
        };
        server.next_scan_at = Some(now + delay);
        self.add_server(server, false).await;
    }

    pub async fn get_queue_sizes(&self) -> (usize, usize, usize) {
        (
            self.hot_queue.lock().await.len(),
            self.warm_queue.lock().await.len(),
            self.cold_queue.lock().await.len(),
        )
    }

    /// Periodically refill queues from DB with servers whose scan interval has elapsed.
    /// Prevents scanner from stalling when in-memory queues drain after initial batch.
    pub async fn try_refill_queues(&self) {
        let now = Utc::now();

        struct QueueCheck {
            queue: Arc<Mutex<VecDeque<ServerTarget>>>,
            priority: i32,
            interval_hours: i64,
            threshold: usize,
            limit: i64,
        }

        let checks = vec![
            QueueCheck { queue: Arc::clone(&self.hot_queue), priority: 1, interval_hours: 4, threshold: 1000, limit: 500 },
            QueueCheck { queue: Arc::clone(&self.warm_queue), priority: 2, interval_hours: 24, threshold: 500, limit: 300 },
            QueueCheck { queue: Arc::clone(&self.cold_queue), priority: 3, interval_hours: 168, threshold: 500, limit: 200 },
        ];

        for check in checks {
            if check.queue.lock().await.len() >= check.threshold { continue; }

            let interval_str = format!("{} hours", check.interval_hours);
            let ready_servers = match sqlx::query_as::<_, crate::db::Server>(
                "SELECT * FROM servers WHERE priority = $1 \
                 AND (last_seen IS NULL OR last_seen < NOW() - CAST($2 AS INTERVAL)) \
                 AND status != 'ignored' \
                 ORDER BY last_seen ASC NULLS FIRST LIMIT $3"
            )
            .bind(check.priority)
            .bind(&interval_str)
            .bind(check.limit)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(s) => s,
                Err(_) => continue,
            };

            if ready_servers.is_empty() { continue; }

            tracing::info!("Queue refill: adding {} priority={} servers from DB", ready_servers.len(), check.priority);
            let mut q = check.queue.lock().await;
            for server in ready_servers {
                let mut target = ServerTarget::new(server.ip, server.port as u16, server.server_type);
                target.priority = check.priority;
                target.last_scanned = server.last_seen.map(|t| t.and_utc());
                target.next_scan_at = None; // Ready to scan now
                q.push_back(target);
            }
        }
    }

    pub async fn load_from_database(&self) -> Result<(), crate::db::DatabaseError> {
        // Only load servers that are currently online or have been online in the past.
        // We filter out 'unknown' status servers which were likely just potential discovery targets.
        let servers = sqlx::query_as::<_, crate::db::Server>(
            "SELECT * FROM servers WHERE status != 'unknown' AND (status = 'online' OR motd IS NOT NULL) ORDER BY priority ASC, last_seen ASC LIMIT 50000",
        )
        .fetch_all(self.db.pool())
        .await?;

        for server in servers {
            let mut target = ServerTarget::new(server.ip, server.port as u16, server.server_type);
            target.priority = server.priority;
            target.consecutive_failures = server.consecutive_failures;
            target.category = AsnCategory::Unknown;

            if let Some(last_seen) = server.last_seen {
                let last = last_seen.and_utc();
                target.last_scanned = Some(last);
                let delay = match target.priority {
                    1 => chrono::Duration::hours(4),
                    2 => chrono::Duration::hours(24),
                    _ => chrono::Duration::days(7),
                };
                target.next_scan_at = Some(last + delay);
            }
            self.add_server(target, false).await;
        }
        Ok(())
    }

    pub async fn select_next_asn_for_warm_scan(&self) -> Option<String> {
        let counts = self.asn_scan_counts.lock().await;
        let last_scanned = self.asn_last_scanned.lock().await;
        
        let hosting_asns: Vec<crate::asn::AsnRecord> = self.db.get_asns_by_category("hosting").await.unwrap_or_default();
        let mut candidates = {
            let mut c = Vec::new();
            for record in hosting_asns {
                let score = self.calculate_asn_score(&record.asn, &counts, &last_scanned).await;
                c.push((record.asn, score));
            }
            c
        };

        if candidates.is_empty() { return None; }

        // Sort by score descending and take top 10 for weighted selection
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top_n = candidates.into_iter().take(10).collect::<Vec<_>>();
        
        // Weighted random selection to ensure diversity
        let mut rng = rand::thread_rng();
        // Use a simple weight: max(0.1, score + 5.0) to ensure even low scores have a chance but high scores are preferred
        top_n.choose_weighted(&mut rng, |item| (item.1 + 5.0).max(0.1)).ok().map(|item| item.0.clone())
    }

    async fn calculate_asn_score(
        &self, 
        asn: &str, 
        counts: &std::collections::HashMap<String, u32>,
        last_scanned: &std::collections::HashMap<String, DateTime<Utc>>
    ) -> f32 {
        let scan_count = counts.get(asn).copied().unwrap_or(0) as f32;
        let hours_since = last_scanned.get(asn)
            .map(|last| Utc::now().signed_duration_since(*last).num_hours() as f32)
            .unwrap_or(168.0); // 7 days default

        // Time score: 0 to 10 (older is higher)
        let time_score = (hours_since / 24.0).min(10.0);
        
        // Frequency penalty: more aggressive to prevent stalling
        // Each scan attempt (200 IPs) reduces score by 1.0
        let frequency_penalty = (scan_count * 1.0).min(8.0);
        
        time_score - frequency_penalty
    }

    pub async fn record_asn_scan(&self, asn: &str) {
        let mut counts = self.asn_scan_counts.lock().await;
        *counts.entry(asn.to_string()).or_insert(0) += 1;
        let mut scanned = self.asn_last_scanned.lock().await;
        *scanned.entry(asn.to_string()).or_insert(Utc::now()) = Utc::now();
    }
}
