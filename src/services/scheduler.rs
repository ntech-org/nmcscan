//! Priority-based scheduler for efficient server scanning.
//!
//! Implements Hot/Warm/Cold tier algorithm with ethical weighted selection.
//! Discovery uses deterministic hash-based IP shuffle tracked by offset in PostgreSQL.

use crate::models::asn::AsnCategory;
use crate::repositories::{AsnRepository, ServerRepository};
use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Queue sizes for API reporting.
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    pub hot: usize,
    pub warm: usize,
    pub cold: usize,
    pub discovery: usize,
}

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
        self.success_rate =
            (self.success_rate * (self.scan_count - 1) as f32 + 1.0) / self.scan_count as f32;
    }

    pub fn mark_offline(&mut self) {
        self.consecutive_failures += 1;
        self.scan_count += 1;
        self.success_rate =
            (self.success_rate * (self.scan_count - 1) as f32) / self.scan_count as f32;

        if self.consecutive_failures > 5 {
            self.priority = 3; // Move to Cold
        }
    }
}

/// Deterministic hash-based shuffle: compute the IP at a given position
/// in a shuffled permutation of a CIDR range.
///
/// Uses a linear bijection: f(x) = (a*x + b) mod range_size
/// where gcd(a, range_size) = 1 (guaranteed by construction).
/// Same (position, seed) → same IP every time. No storage needed.
pub fn ip_at_position(position: u64, base_ip: u32, range_size: u64, seed: u64) -> u32 {
    if range_size <= 1 {
        return base_ip;
    }

    // Derive a from seed, then reduce mod range_size to avoid u64 overflow in multiply.
    // Must be coprime to range_size for bijection.
    let mut a = mix_bits(seed ^ 0x9E3779B97F4A7C15) % range_size;
    if a % 2 == 0 {
        a ^= 1; // make odd for better coprimality with powers of 2
    }
    while gcd(a, range_size) != 1 {
        a = (a + 1) % range_size;
        if a == 0 {
            a = 1;
        }
    }
    let b = mix_bits(seed ^ 0x517CC1B727220A95) % range_size;

    // Bijective mapping: f(x) = (a*x + b) mod range_size
    let permuted = (a * position + b) % range_size;

    base_ip + permuted as u32
}

/// Greatest common divisor.
#[inline]
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Mix a u64 value using multiply-xorshift for good avalanche properties.
#[inline]
fn mix_bits(x: u64) -> u64 {
    let mut v = x;
    v ^= v >> 33;
    v = v.wrapping_mul(0xFF51AFD7ED558CCD);
    v ^= v >> 33;
    v = v.wrapping_mul(0xC4CEB9FE1A85EC53);
    v ^= v >> 33;
    v
}

/// Maximum items per queue to prevent unbounded memory growth.
const MAX_QUEUE_SIZE: usize = 10000;

pub struct Scheduler {
    hot_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    warm_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    cold_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    /// Discovery targets — always ready (next_scan_at = None).
    /// Separate from main queues to prevent stalls when queues are full.
    discovery_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    pub server_repo: Arc<ServerRepository>,
    pub asn_repo: Arc<AsnRepository>,
    pub test_mode: bool,
    test_interval: u32,
}

impl Scheduler {
    pub fn new(
        server_repo: Arc<ServerRepository>,
        asn_repo: Arc<AsnRepository>,
        test_mode: bool,
        test_interval: u32,
    ) -> Self {
        Self {
            hot_queue: Arc::new(Mutex::new(VecDeque::new())),
            warm_queue: Arc::new(Mutex::new(VecDeque::new())),
            cold_queue: Arc::new(Mutex::new(VecDeque::new())),
            discovery_queue: Arc::new(Mutex::new(VecDeque::new())),
            server_repo,
            asn_repo,
            test_mode,
            test_interval,
        }
    }

    pub async fn add_server(&self, server: ServerTarget, at_front: bool) {
        // Discovery targets always go to the dedicated discovery queue (always ready).
        if server.is_discovery {
            let mut dq = self.discovery_queue.lock().await;
            if dq.len() >= MAX_QUEUE_SIZE {
                return;
            }
            if at_front {
                dq.push_front(server);
            } else {
                dq.push_back(server);
            }
            return;
        }

        let queue = match server.priority {
            1 => &self.hot_queue,
            2 => &self.warm_queue,
            _ => &self.cold_queue,
        };
        let mut q = queue.lock().await;
        if q.len() >= MAX_QUEUE_SIZE {
            return;
        }
        if at_front {
            q.push_front(server);
        } else {
            q.push_back(server);
        }
    }

    /// Get the next server to scan. Picks from the queue whose earliest-ready item
    /// is due first. Never returns a server whose next_scan_at is in the future.
    /// This prevents the resource leak where servers get over-scanned.
    ///
    /// Discovery targets (always ready) are checked first to prevent stalls
    /// when main queues are full of items with future next_scan_at.
    pub async fn next_server(&self) -> Option<ServerTarget> {
        // 1. Check discovery queue first — always ready, O(1) pop
        {
            let mut dq = self.discovery_queue.lock().await;
            if let Some(target) = dq.pop_front() {
                return Some(target);
            }
        }

        let now = Utc::now();

        // 2. For each tier, find the index of the earliest-ready item (up to 5000 deep).
        let hot_info = self.find_earliest_ready(&self.hot_queue, &now).await;
        let warm_info = self.find_earliest_ready(&self.warm_queue, &now).await;
        let cold_info = self.find_earliest_ready(&self.cold_queue, &now).await;

        // Collect ready tiers with their earliest scan time
        let mut ready: Vec<(&Arc<Mutex<VecDeque<ServerTarget>>>, usize, DateTime<Utc>)> =
            Vec::new();
        if let Some((idx, t)) = hot_info {
            ready.push((&self.hot_queue, idx, t));
        }
        if let Some((idx, t)) = warm_info {
            ready.push((&self.warm_queue, idx, t));
        }
        if let Some((idx, t)) = cold_info {
            ready.push((&self.cold_queue, idx, t));
        }

        if !ready.is_empty() {
            // Pick the tier with the earliest-ready item (most overdue)
            ready.sort_by_key(|(_, _, t)| *t);
            let (queue, idx, _) = &ready[0];
            return Some(queue.lock().await.remove(*idx).unwrap());
        }

        // 3. Nothing found in first 5000 — deep scan ALL queues for ANY ready item.
        let queues = [&self.hot_queue, &self.warm_queue, &self.cold_queue];
        let mut best: Option<(&Arc<Mutex<VecDeque<ServerTarget>>>, usize, DateTime<Utc>)> = None;

        for queue in &queues {
            let q = queue.lock().await;
            for i in 0..q.len() {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    let t = q[i].next_scan_at.unwrap_or(now);
                    if best.as_ref().map_or(true, |(_, _, bt)| t < *bt) {
                        best = Some((queue, i, t));
                    }
                }
            }
        }

        if let Some((queue, idx, _)) = best {
            return Some(queue.lock().await.remove(idx).unwrap());
        }

        // Truly nothing ready — return None so the scanner loop can sleep.
        None
    }

    /// Search up to `limit` items in a queue for the earliest one ready to scan.
    /// Returns (index, next_scan_at) of the best candidate, or None if none ready.
    async fn find_earliest_ready(
        &self,
        queue: &Arc<Mutex<VecDeque<ServerTarget>>>,
        now: &DateTime<Utc>,
    ) -> Option<(usize, DateTime<Utc>)> {
        let q = queue.lock().await;
        let limit = std::cmp::min(q.len(), 5000);
        let mut best_idx = None;
        let mut best_time = None;

        for i in 0..limit {
            let ready = q[i].next_scan_at.map_or(true, |t| t <= *now);
            if ready {
                let t = q[i].next_scan_at.unwrap_or(*now);
                if best_time.is_none() || t < best_time.unwrap() {
                    best_idx = Some(i);
                    best_time = Some(t);
                }
            }
        }

        best_idx.map(|i| (i, best_time.unwrap()))
    }

    pub async fn fill_warm_queue_if_needed(&self) {
        // Always run discovery — discovery targets go to the dedicated
        // discovery_queue, so queue size doesn't matter.
        // This prevents the stall where all main queue items have future next_scan_at.

        // INTERLEAVED DISCOVERY: Fetch hosting ranges
        match self.asn_repo.get_ranges_to_scan("hosting", 100).await {
            Ok(ranges) => {
                if ranges.is_empty() {
                    return;
                }
                let count = self.fill_discovery_queue(ranges, 2, 100).await.unwrap_or(0);
                if count > 0 {
                    tracing::info!(
                        "Discovery: Added {} new targets to discovery queue (from hosting)",
                        count
                    );
                } else {
                    // Dead tick: all ranges were exhausted. Re-fetch immediately
                    // — ranges now have offset=0 with fresh epoch.
                    match self.asn_repo.get_ranges_to_scan("hosting", 100).await {
                        Ok(ranges2) if !ranges2.is_empty() => {
                            let count2 = self
                                .fill_discovery_queue(ranges2, 2, 100)
                                .await
                                .unwrap_or(0);
                            if count2 > 0 {
                                tracing::info!("Discovery: Recovered from dead tick, added {} targets (hosting)", count2);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                tracing::error!("Discovery error: Failed to fetch hosting ranges: {}", e);
            }
        }
    }

    pub async fn fill_cold_queue_if_needed(&self) {
        // Always run discovery — discovery targets go to the dedicated discovery_queue.

        // 1. Try to recycle dead/ignored servers (these go to main cold queue)
        if let Ok(dead_servers) = self.server_repo.get_dead_servers(1000).await {
            for server in dead_servers {
                let mut target =
                    ServerTarget::new(server.ip.to_string(), server.port as i32 as u16, server.server_type);
                target.priority = 3;
                if let Some(last) = server.last_seen {
                    target.next_scan_at = Some(last.and_utc() + chrono::Duration::days(7));
                }
                self.add_server(target, false).await;
            }
        }

        // 2. INTERLEAVED DISCOVERY: Fetch residential and unknown ranges
        let mut ranges = self
            .asn_repo
            .get_ranges_to_scan("residential", 100)
            .await
            .unwrap_or_default();
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
                tracing::info!(
                    "Discovery: Added {} new targets to discovery queue (from {})",
                    count,
                    source
                );
            } else {
                // Dead tick recovery: re-fetch ranges (now offset=0 with fresh epoch)
                let mut ranges2 = self
                    .asn_repo
                    .get_ranges_to_scan("residential", 100)
                    .await
                    .unwrap_or_default();
                let mut source2 = "residential";
                if ranges2.is_empty() {
                    if let Ok(r) = self.asn_repo.get_ranges_to_scan("unknown", 100).await {
                        ranges2 = r;
                        source2 = "unknown";
                    }
                }
                if !ranges2.is_empty() {
                    let count2 = self
                        .fill_discovery_queue(ranges2, 3, 100)
                        .await
                        .unwrap_or(0);
                    if count2 > 0 {
                        tracing::info!(
                            "Discovery: Recovered from dead tick, added {} targets ({})",
                            count2,
                            source2
                        );
                    }
                }
            }
        }
    }

    /// Master discovery function that takes multiple ranges, computes IPs via
    /// deterministic shuffle at the current offset, and pushes to the queue.
    ///
    /// No bitset filter — every IP at each offset is unique per epoch cycle.
    /// When a range is exhausted, its offset resets to 0 and epoch increments,
    /// giving a fresh shuffled permutation for the next cycle.
    pub async fn fill_discovery_queue(
        &self,
        ranges: Vec<crate::models::entities::asn_ranges::Model>,
        priority: i32,
        ips_per_range: usize,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        use ipnetwork::Ipv4Network;

        let mut all_targets = Vec::new();
        let mut updates = Vec::new();

        for range in ranges {
            let network: Ipv4Network = match range.cidr.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let total_ips = network.size() as i64;
            let current_offset = range.scan_offset;

            // If the range is exhausted, reset it immediately for next cycle.
            // This prevents wasted batch slots — without this, exhausted ranges
            // returned by get_ranges_to_scan contribute nothing.
            if current_offset >= total_ips {
                updates.push((range.cidr.clone(), 0_i64, true, true));
                continue;
            }

            // Compute seed from CIDR + epoch for deterministic shuffle
            let seed = compute_seed(&range.cidr, range.scan_epoch as u64);
            let base_ip = u32::from(network.network());

            // Compute IPs via deterministic shuffle at current offset
            let mut added = 0u64;
            let offset = current_offset as u64;
            while (added as usize) < ips_per_range && (offset + added) < total_ips as u64 {
                let position = offset + added;
                let ip_val = ip_at_position(position, base_ip, total_ips as u64, seed);
                let ip_str = format_ip(ip_val);

                let mut java_target = ServerTarget::new(ip_str.clone(), 25565, "java".to_string());
                java_target.category = if priority == 2 {
                    AsnCategory::Hosting
                } else {
                    AsnCategory::Residential
                };
                java_target.priority = priority;
                java_target.is_discovery = true;
                all_targets.push(java_target);

                let mut bedrock_target = ServerTarget::new(ip_str, 19132, "bedrock".to_string());
                bedrock_target.category = if priority == 2 {
                    AsnCategory::Hosting
                } else {
                    AsnCategory::Residential
                };
                bedrock_target.priority = priority;
                bedrock_target.is_discovery = true;
                all_targets.push(bedrock_target);

                added += 1;
            }

            if added == 0 {
                // Range had IPs available but none were generated (shouldn't happen,
                // but safety net). Reset with epoch bump to get fresh permutation.
                updates.push((range.cidr.clone(), 0_i64, true, true));
                continue;
            }

            let new_offset = offset + added;
            let is_done = new_offset >= total_ips as u64;
            updates.push((
                range.cidr.clone(),
                if is_done { 0 } else { new_offset as i64 },
                is_done,
                is_done, // increment epoch when range is exhausted
            ));
        }

        // Batch update progress in DB (offset + epoch)
        if !updates.is_empty() {
            self.asn_repo.update_batch_range_progress(updates).await?;
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
                        let mut t_up = ServerTarget::new(
                            server.ip.clone(),
                            server.port + 1,
                            "java".to_string(),
                        );
                        t_up.direction = 1;
                        t_up.category = server.category.clone();
                        t_up.priority = server.priority;
                        self.add_server(t_up, false).await;

                        let mut t_down = ServerTarget::new(
                            server.ip.clone(),
                            server.port - 1,
                            "java".to_string(),
                        );
                        t_down.direction = -1;
                        t_down.category = server.category.clone();
                        t_down.priority = server.priority;
                        self.add_server(t_down, false).await;
                    }
                } else if server.direction == 1 && server.port < 65535 {
                    // Continue scanning upwards
                    let mut t_up =
                        ServerTarget::new(server.ip.clone(), server.port + 1, "java".to_string());
                    t_up.direction = 1;
                    t_up.category = server.category.clone();
                    t_up.priority = server.priority;
                    self.add_server(t_up, false).await;
                } else if server.direction == -1 && server.port > 1 {
                    // Continue scanning downwards
                    let mut t_down =
                        ServerTarget::new(server.ip.clone(), server.port - 1, "java".to_string());
                    t_down.direction = -1;
                    t_down.category = server.category.clone();
                    t_down.priority = server.priority;
                    self.add_server(t_down, false).await;
                }
            }
        } else {
            server.mark_offline();
        }

        // If it's a new discovery target and it's offline, don't re-queue it.
        // This prevents the memory queue from being filled with thousands of offline IPs.
        if is_new_discovery && !was_online {
            tracing::debug!(
                "Dropping offline discovery target: {}:{}",
                server.ip.to_string(),
                server.port
            );
            return;
        }

        let delay = if self.test_mode {
            chrono::Duration::seconds(self.test_interval as i64)
        } else {
            match server.priority {
                1 => chrono::Duration::hours(2),
                2 => chrono::Duration::hours(24),
                _ => chrono::Duration::days(7),
            }
        };
        server.next_scan_at = Some(now + delay);
        self.add_server(server, false).await;
    }

    pub async fn get_queue_sizes(&self) -> (usize, usize, usize, usize) {
        (
            self.hot_queue.lock().await.len(),
            self.warm_queue.lock().await.len(),
            self.cold_queue.lock().await.len(),
            self.discovery_queue.lock().await.len(),
        )
    }

    /// Get queue sizes as a serializable struct for the API.
    pub async fn get_queue_stats(&self) -> QueueStats {
        let (hot, warm, cold, discovery) = self.get_queue_sizes().await;
        QueueStats {
            hot,
            warm,
            cold,
            discovery,
        }
    }

    /// Periodically refill queues from DB with servers whose scan interval has elapsed.
    /// Only runs when queue is below 25% of threshold to prevent aggressive re-scanning.
    pub async fn try_refill_queues(&self) {
        let configs = vec![
            (1, 2, 1000, 500u64), // priority, interval_hours, threshold, limit
            (2, 24, 500, 300u64),
            (3, 168, 500, 200u64),
        ];

        for (priority, interval_hours, threshold, limit) in configs {
            let queue = match priority {
                1 => &self.hot_queue,
                2 => &self.warm_queue,
                _ => &self.cold_queue,
            };

            let current_len = queue.lock().await.len();
            // Only refill when queue is below 25% of threshold
            let refill_threshold = threshold / 4;
            if current_len >= refill_threshold {
                continue;
            }

            let ready_servers = match self
                .server_repo
                .get_servers_for_refill(priority, interval_hours, limit)
                .await
            {
                Ok(s) => s,
                Err(_) => continue,
            };

            if ready_servers.is_empty() {
                continue;
            }

            tracing::info!(
                "Queue refill: priority={} queue has {} items (threshold: {}), adding {} servers from DB",
                priority, current_len, refill_threshold, ready_servers.len()
            );
            let mut q = queue.lock().await;
            for server in ready_servers {
                let mut target = ServerTarget::new(
                    server.ip.to_string(),
                    server.port.try_into().unwrap_or(25565),
                    server.server_type,
                );
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
            let mut target = ServerTarget::new(
                server.ip.to_string(),
                server.port.try_into().unwrap_or(25565),
                server.server_type,
            );
            target.priority = server.priority;
            target.consecutive_failures = server.consecutive_failures;
            target.category = AsnCategory::Unknown;

            if let Some(last_seen) = server.last_seen {
                let last = last_seen.and_utc();
                target.last_scanned = Some(last);
                let delay = match target.priority {
                    1 => chrono::Duration::hours(2),
                    2 => chrono::Duration::hours(24),
                    _ => chrono::Duration::days(7),
                };
                target.next_scan_at = Some(last + delay);
            }
            self.add_server(target, false).await;
        }
        Ok(())
    }
}

/// Compute a deterministic seed from a CIDR string and epoch.
fn compute_seed(cidr: &str, epoch: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cidr.hash(&mut hasher);
    epoch.hash(&mut hasher);
    hasher.finish()
}

/// Format a u32 IP value as a dotted-quad string.
fn format_ip(ip: u32) -> String {
    let a = (ip >> 24) & 0xFF;
    let b = (ip >> 16) & 0xFF;
    let c = (ip >> 8) & 0xFF;
    let d = ip & 0xFF;
    format!("{}.{}.{}.{}", a, b, c, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_at_position_covers_range() {
        let base = 0x0A000000u32; // 10.0.0.0
        let size = 256u64;
        let seed = compute_seed("10.0.0.0/24", 0);

        let mut seen = std::collections::HashSet::new();
        for pos in 0..size {
            let ip = ip_at_position(pos, base, size, seed);
            assert!(
                ip >= base && ip < base + size as u32,
                "IP out of range at pos {}",
                pos
            );
            assert!(
                seen.insert(ip),
                "Duplicate IP {} at pos {}",
                format_ip(ip),
                pos
            );
        }
        assert_eq!(seen.len(), size as usize, "Should cover all {} IPs", size);
    }

    #[test]
    fn test_ip_at_position_different_seeds() {
        let base = 0x0A000000u32;
        let size = 256u64;
        let seed0 = compute_seed("10.0.0.0/24", 0);
        let seed1 = compute_seed("10.0.0.0/24", 1);

        let ip0 = ip_at_position(0, base, size, seed0);
        let ip1 = ip_at_position(0, base, size, seed1);
        assert_ne!(ip0, ip1, "Different seeds should produce different IPs");
    }

    #[test]
    fn test_ip_at_position_deterministic() {
        let base = 0x0A000000u32;
        let size = 256u64;
        let seed = compute_seed("10.0.0.0/24", 0);

        let a = ip_at_position(42, base, size, seed);
        let b = ip_at_position(42, base, size, seed);
        assert_eq!(a, b);
    }

    #[test]
    fn test_ip_at_position_large_range() {
        // /16 range: 65536 IPs
        let base = 0x0A000000u32;
        let size = 65536u64;
        let seed = compute_seed("10.0.0.0/16", 0);

        // Spot check: all should be in range
        for pos in [0, 1000, 32768, 65535] {
            let ip = ip_at_position(pos, base, size, seed);
            assert!(
                ip >= base && ip < base + size as u32,
                "IP out of range at pos {}",
                pos
            );
        }

        // Verify first 1000 are unique
        let mut seen = std::collections::HashSet::new();
        for pos in 0..1000u64 {
            let ip = ip_at_position(pos, base, size, seed);
            assert!(
                seen.insert(ip),
                "Duplicate IP {} at pos {}",
                format_ip(ip),
                pos
            );
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn test_ip_at_position_non_power_of_2() {
        // 100 IPs (not a power of 2)
        let base = 0x0A000000u32;
        let size = 100u64;
        let seed = compute_seed("10.0.0.0/25", 0);

        let mut seen = std::collections::HashSet::new();
        for pos in 0..size {
            let ip = ip_at_position(pos, base, size, seed);
            assert!(
                ip >= base && ip < base + size as u32,
                "IP out of range at pos {}",
                pos
            );
            assert!(
                seen.insert(ip),
                "Duplicate IP {} at pos {}",
                format_ip(ip),
                pos
            );
        }
        assert_eq!(seen.len(), size as usize);
    }

    #[test]
    fn test_ip_at_position_very_small() {
        // 3 IPs
        let base = 0x0A000000u32;
        let size = 3u64;
        let seed = compute_seed("test", 0);

        let mut seen = std::collections::HashSet::new();
        for pos in 0..size {
            let ip = ip_at_position(pos, base, size, seed);
            assert!(
                ip >= base && ip < base + size as u32,
                "IP out of range at pos {}",
                pos
            );
            assert!(seen.insert(ip), "Duplicate IP at pos {}", pos);
        }
    }

    #[test]
    fn test_format_ip() {
        assert_eq!(format_ip(0x0A000001), "10.0.0.1");
        assert_eq!(format_ip(0xC0A80001), "192.168.0.1");
        assert_eq!(format_ip(0), "0.0.0.0");
        assert_eq!(format_ip(0xFFFFFFFF), "255.255.255.255");
    }

    #[test]
    fn test_compute_seed_varies() {
        let s1 = compute_seed("10.0.0.0/24", 0);
        let s2 = compute_seed("10.0.0.0/24", 1);
        let s3 = compute_seed("10.0.0.1/24", 0);
        assert_ne!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_epoch_changes_permutation() {
        let base = 0x0A000000u32;
        let size = 256u64;
        let seed0 = compute_seed("10.0.0.0/24", 0);
        let seed1 = compute_seed("10.0.0.0/24", 1);

        // Different epochs should produce different permutations
        let mut diffs = 0;
        for pos in 0..size {
            if ip_at_position(pos, base, size, seed0) != ip_at_position(pos, base, size, seed1) {
                diffs += 1;
            }
        }
        // Most positions should differ
        assert!(diffs > size / 2, "Only {}/{} positions differ", diffs, size);
    }
}
