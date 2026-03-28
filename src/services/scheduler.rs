//! Priority-based scheduler for efficient server scanning.
//!
//! Implements Hot/Warm/Cold tier algorithm with ethical weighted selection.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use crate::repositories::{ServerRepository, AsnRepository};
use crate::models::asn::AsnCategory;
use rand::prelude::*;
use rand::seq::SliceRandom;

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
    pub server_repo: Arc<ServerRepository>,
    pub asn_repo: Arc<AsnRepository>,
    pub test_mode: bool,
    test_interval: u32,
}

impl Scheduler {
    pub fn new(server_repo: Arc<ServerRepository>, asn_repo: Arc<AsnRepository>, test_mode: bool, test_interval: u32) -> Self {
        Self {
            hot_queue: Arc::new(Mutex::new(VecDeque::new())),
            warm_queue: Arc::new(Mutex::new(VecDeque::new())),
            cold_queue: Arc::new(Mutex::new(VecDeque::new())),
            server_repo,
            asn_repo,
            test_mode,
            test_interval,
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
        
        // Tier sizes for dynamic priority
        let hot_len = self.hot_queue.lock().await.len();
        let warm_len = self.warm_queue.lock().await.len();

        let roll = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..100)
        };

        // If Hot or Warm have items, they get 90% of the priority.
        // Hot gets first pick in that 90%.
        let tiers = if (hot_len > 0 || warm_len > 0) && roll < 90 {
            if hot_len > 0 {
                vec![1, 2, 3]
            } else {
                vec![2, 1, 3]
            }
        } else if roll < 95 {
            vec![2, 1, 3] // 5% dedicated to Warm
        } else {
            vec![3, 1, 2] // 5% dedicated to Cold
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
        if warm_len > 5000 { return; } 

        // INTERLEAVED DISCOVERY: Fetch hosting ranges
        match self.asn_repo.get_ranges_to_scan("hosting", 100).await {
            Ok(ranges) => {
                if ranges.is_empty() {
                    return;
                }
                let count = self.fill_discovery_queue(ranges, 2, 100).await.unwrap_or(0);
                if count > 0 {
                    tracing::info!("Discovery: Added {} new targets to Warm queue (from hosting)", count);
                }
            }
            Err(e) => {
                tracing::error!("Discovery error: Failed to fetch hosting ranges: {}", e);
            }
        }
    }

    pub async fn fill_cold_queue_if_needed(&self) {
        let cold_len = self.cold_queue.lock().await.len();
        if cold_len > 5000 { return; }

        // 1. Try to recycle dead/ignored servers
        if let Ok(dead_servers) = self.server_repo.get_dead_servers(1000).await {
            for server in dead_servers {
                let mut target = ServerTarget::new(server.ip, server.port as u16, server.server_type);
                target.priority = 3;
                if let Some(last) = server.last_seen {
                    target.next_scan_at = Some(last.and_utc() + chrono::Duration::days(7));
                }
                self.add_server(target, false).await;
            }
        }

        // 2. INTERLEAVED DISCOVERY: Fetch residential and unknown ranges
        let mut ranges = self.asn_repo.get_ranges_to_scan("residential", 100).await.unwrap_or_default();
        let mut source = "residential";
        if ranges.is_empty() {
            if let Ok(r) = self.asn_repo.get_ranges_to_scan("unknown", 100).await {
                ranges = r;
                source = "unknown";
            }
        }

        if !ranges.is_empty() {
            let count = self.fill_discovery_queue(ranges, 3, 100).await.unwrap_or(0);
            if count > 0 {
                tracing::info!("Discovery: Added {} new targets to Cold queue (from {})", count, source);
            }
        }
    }

    /// Master discovery function that takes multiple ranges, pulls a few IPs from each,
    /// shuffles the aggregate list, and pushes to the queue.
    pub async fn fill_discovery_queue(
        &self,
        ranges: Vec<crate::models::entities::asn_ranges::Model>,
        priority: i32,
        ips_per_range: usize,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        use ipnetwork::Ipv4Network;

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

            // If the range is already exhausted, mark it for reset and skip
            if current_offset >= total_ips {
                updates.push((range.cidr.clone(), 0, true));
                continue;
            }

            // Pull a batch of IPs from the range
            while added < ips_per_range && current_offset < total_ips {
                if let Some(ip) = network.nth(current_offset as u32) {
                    all_candidates.push(ip.to_string());
                    added += 1;
                }
                current_offset += 1;
            }

            // Always record progress to move to the next set of IPs or next range
            let is_done = current_offset >= total_ips;
            updates.push((range.cidr.clone(), if is_done { 0 } else { current_offset }, is_done));
        }

        // Batch update progress in DB
        if !updates.is_empty() {
            self.asn_repo.update_batch_range_progress(updates).await?;
        }

        if all_candidates.is_empty() { return Ok(0); }

        // BATCH CHECK: Find which IPs are already in our database in one go
        let known_ips = self.server_repo.get_existing_ips(all_candidates.clone()).await?;

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

        let added_count = all_targets.len();

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

        Ok(added_count)
    }
    /// Synchronous helper to sample IPs to avoid holding non-Send ThreadRng across awaits.
    #[allow(dead_code)]
    fn sample_ips_from_network(&self, network: &ipnetwork::Ipv4Network, max_to_add: usize) -> Vec<std::net::Ipv4Addr> {
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
        let configs = vec![
            (1, 4, 1000, 500u64), // priority, interval_hours, threshold, limit
            (2, 24, 500, 300u64),
            (3, 168, 500, 200u64),
        ];

        for (priority, interval_hours, threshold, limit) in configs {
            let queue = match priority {
                1 => &self.hot_queue,
                2 => &self.warm_queue,
                _ => &self.cold_queue,
            };

            if queue.lock().await.len() >= threshold { continue; }

            let ready_servers = match self.server_repo.get_servers_for_refill(priority, interval_hours, limit).await {
                Ok(s) => s,
                Err(_) => continue,
            };

            if ready_servers.is_empty() { continue; }

            tracing::info!("Queue refill: adding {} priority={} servers from DB", ready_servers.len(), priority);
            let mut q = queue.lock().await;
            for server in ready_servers {
                let mut target = ServerTarget::new(server.ip, server.port.try_into().unwrap_or(25565), server.server_type);
                target.priority = priority;
                target.last_scanned = server.last_seen.map(|t| t.and_utc());
                target.next_scan_at = None; // Ready to scan now
                q.push_back(target);
            }
        }
    }

    pub async fn load_from_database(&self) -> Result<(), sea_orm::DbErr> {
        // Only load servers that are currently online or have been online in the past.
        // We filter out 'unknown' status servers which were likely just potential discovery targets.
        let servers = self.server_repo.get_servers_for_load(50000).await?;

        for server in servers {
            let mut target = ServerTarget::new(server.ip, server.port.try_into().unwrap_or(25565), server.server_type);
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

    #[allow(dead_code)]
    pub async fn select_next_asn_for_warm_scan(&self) -> Option<String> {
        None
    }

    #[allow(dead_code)]
    async fn calculate_asn_score(
        &self, 
        _asn: &str, 
        _counts: &std::collections::HashMap<String, u32>,
        _last_scanned: &std::collections::HashMap<String, DateTime<Utc>>
    ) -> f32 {
        0.0
    }

    #[allow(dead_code)]
    pub async fn record_asn_scan(&self, _asn: &str) {
    }
}
