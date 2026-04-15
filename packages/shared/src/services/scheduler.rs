//! Simplified priority-based scheduler for efficient server scanning.
//!
//! Uses a single unified priority queue instead of the previous 4-queue system.
//! Discovery targets are generated in bulk and pushed directly to the queue.
//! No more complex refill logic — servers are scheduled with next_scan_at timestamps.

use crate::models::asn::AsnCategory;
use crate::repositories::{AsnRepository, ServerRepository};
use chrono::{DateTime, Utc};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Queue sizes for API reporting.
/// Keeps backward compatibility with old fields (hot/warm/cold) while supporting
/// the new unified queue model.
#[derive(Debug, Clone, Serialize)]
pub struct QueueStats {
    pub discovery: usize, // Discovery dedup count
    pub total: usize,     // New: total queue size
    pub ready: usize,     // New: ready count
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
}

impl ServerTarget {
    pub fn new(ip: String, port: u16, server_type: String) -> Self {
        Self {
            ip,
            port,
            hostname: None,
            priority: 2,
            category: AsnCategory::Unknown,
            last_scanned: None,
            next_scan_at: None,
            consecutive_failures: 0,
            scan_count: 0,
            success_rate: 0.0,
            server_type,
            is_discovery: false,
        }
    }

    pub fn mark_online(&mut self) {
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
    }
}

// For BinaryHeap: higher priority = scanned sooner, earlier next_scan_at = scanned sooner.
// BinaryHeap is a max-heap, so we reverse the ordering.
impl PartialEq for ServerTarget {
    fn eq(&self, other: &Self) -> bool {
        self.ip == other.ip && self.port == other.port
    }
}

impl Eq for ServerTarget {}

impl PartialOrd for ServerTarget {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for ServerTarget {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Priority 1 (hot) > 2 (warm) > 3 (cold)
        // For same priority, earlier next_scan_at comes first
        let now = Utc::now();
        let self_ready = self.next_scan_at.map_or(true, |t| t <= now);
        let other_ready = other.next_scan_at.map_or(true, |t| t <= now);

        // Ready items always come before non-ready
        match (self_ready, other_ready) {
            (true, false) => return CmpOrdering::Greater,
            (false, true) => return CmpOrdering::Less,
            _ => {}
        }

        // Both ready or both not ready: compare by priority (lower = higher priority)
        match self.priority.cmp(&other.priority) {
            CmpOrdering::Equal => {}
            ord => return ord.reverse(), // reverse because BinaryHeap is max-heap
        }

        // Same priority: earlier next_scan_at first
        let self_time = self.next_scan_at.unwrap_or(DateTime::<Utc>::MIN_UTC);
        let other_time = other.next_scan_at.unwrap_or(DateTime::<Utc>::MIN_UTC);
        self_time.cmp(&other_time).reverse()
    }
}

/// Maximum items in the unified queue to prevent unbounded memory growth.
const MAX_QUEUE_SIZE: usize = 500_000;

/// TTL for discovery deduplication: 3 hours.
const DISCOVERY_DEDUP_TTL_SECS: u64 = 10800;

pub struct Scheduler {
    /// Unified priority queue — always sorted by priority + readiness.
    queue: Arc<Mutex<std::collections::BinaryHeap<ServerTarget>>>,

    /// Discovery deduplication: tracks recently-added IP:port combinations.
    discovery_dedup: Arc<Mutex<HashMap<String, Instant>>>,

    /// Known server IP:port combinations from the database.
    /// Used by discovery to skip IPs that are already known servers.
    known_servers: Arc<Mutex<HashSet<String>>>,

    pub server_repo: Arc<ServerRepository>,
    pub asn_repo: Arc<AsnRepository>,
    pub test_mode: bool,
    test_interval: u32,
    /// Target RPS — used to scale discovery IP generation.
    target_rps: u64,
}

impl Scheduler {
    pub fn new(
        server_repo: Arc<ServerRepository>,
        asn_repo: Arc<AsnRepository>,
        test_mode: bool,
        test_interval: u32,
        target_rps: u64,
    ) -> Self {
        Self {
            queue: Arc::new(Mutex::new(std::collections::BinaryHeap::with_capacity(
                100_000,
            ))),
            discovery_dedup: Arc::new(Mutex::new(HashMap::new())),
            known_servers: Arc::new(Mutex::new(HashSet::new())),
            server_repo,
            asn_repo,
            test_mode,
            test_interval,
            target_rps,
        }
    }

    /// Load all known server IP:port combinations from the database at startup.
    pub async fn load_known_servers(&self) -> Result<usize, sea_orm::DbErr> {
        let servers = self.server_repo.get_all_known_servers().await?;
        let mut known = self.known_servers.lock().await;
        known.clear();
        for server in &servers {
            known.insert(format!("{}:{}", server.ip, server.port));
        }
        let count = known.len();
        tracing::info!("Loaded {} known server IPs into discovery skip-list", count);
        Ok(count)
    }

    /// Register a newly discovered server so future discovery cycles skip it.
    pub async fn register_known_server(&self, ip: &str, port: u16) {
        let key = format!("{}:{}", ip, port);
        self.known_servers.lock().await.insert(key);
    }

    /// Add a server to the priority queue.
    pub async fn add_server(&self, server: ServerTarget) {
        // Discovery targets always go through discovery dedup.
        if server.is_discovery {
            let key = format!("{}:{}", server.ip, server.port);
            let mut dedup = self.discovery_dedup.lock().await;

            let now = Instant::now();
            dedup.retain(|_, instant| {
                now.duration_since(*instant).as_secs() < DISCOVERY_DEDUP_TTL_SECS
            });

            if dedup.contains_key(&key) {
                return;
            }
            dedup.insert(key, now);
            drop(dedup);
        }

        let mut q = self.queue.lock().await;
        if q.len() >= MAX_QUEUE_SIZE {
            return;
        }
        q.push(server);
    }

    /// Add multiple servers at once (batch add for discovery).
    pub async fn add_servers_batch(&self, servers: Vec<ServerTarget>) {
        let mut q = self.queue.lock().await;
        let remaining = MAX_QUEUE_SIZE.saturating_sub(q.len());
        for server in servers.into_iter().take(remaining) {
            q.push(server);
        }
    }

    /// Get the next server to scan. Returns None if no servers are ready.
    /// The BinaryHeap is always sorted by priority + readiness, so we just pop.
    pub async fn next_server(&self) -> Option<ServerTarget> {
        let now = Utc::now();
        let mut q = self.queue.lock().await;

        // Pop items until we find one that's ready, or the queue is empty.
        // Non-ready items are re-pushed to the back (they'll bubble up naturally).
        let mut deferred = Vec::new();

        while let Some(server) = q.pop() {
            let ready = server.next_scan_at.map_or(true, |t| t <= now);
            if ready {
                // Push back any deferred items before returning
                for s in deferred {
                    q.push(s);
                }
                return Some(server);
            } else {
                deferred.push(server);
                // Limit how many items we defer to avoid O(n) behavior
                if deferred.len() >= 1000 {
                    break;
                }
            }
        }

        // Push deferred items back
        for s in deferred {
            q.push(s);
        }

        None
    }

    /// Fill the discovery queue with new targets from ASN ranges.
    /// This is the PRIMARY source of new servers — called every 15 seconds.
    ///
    /// Generates targets from both hosting and residential ranges,
    /// scaling the number of IPs per range based on target RPS.
    pub async fn fill_discovery_queue(&self) {
        // INTERLEAVED DISCOVERY: Fetch hosting ranges (90% of capacity)
        let hosting_ips_per_range =
            std::cmp::max(1u64, (self.target_rps * 15 * 9 / 10).div_ceil(500));

        match self.asn_repo.get_ranges_to_scan("hosting", 500).await {
            Ok(ranges) => {
                if !ranges.is_empty() {
                    let count = self
                        .generate_and_queue_targets(ranges, 2, hosting_ips_per_range as usize)
                        .await;
                    if count > 0 {
                        tracing::info!(
                            "Discovery: Added {} new targets (from hosting, {} IPs/range)",
                            count,
                            hosting_ips_per_range
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("Discovery error: Failed to fetch hosting ranges: {}", e);
            }
        }

        // Residential ranges (10% of capacity, deliberately limited)
        let residential_ips_per_range =
            std::cmp::max(1u64, (self.target_rps * 15 / 10).div_ceil(500));

        match self.asn_repo.get_ranges_to_scan("residential", 500).await {
            Ok(ranges) => {
                if !ranges.is_empty() {
                    let count = self
                        .generate_and_queue_targets(ranges, 3, residential_ips_per_range as usize)
                        .await;
                    if count > 0 {
                        tracing::info!(
                            "Discovery: Added {} new targets (from residential, {} IPs/range)",
                            count,
                            residential_ips_per_range
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("Discovery error: Failed to fetch residential ranges: {}", e);
            }
        }
    }

    /// Generate targets from ASN ranges and push them to the queue.
    /// Uses deterministic shuffle for IP generation.
    async fn generate_and_queue_targets(
        &self,
        ranges: Vec<crate::models::entities::asn_ranges::Model>,
        priority: i32,
        ips_per_range: usize,
    ) -> usize {
        use ipnetwork::Ipv4Network;

        let mut all_targets = Vec::new();
        let mut updates = Vec::new();
        let mut ranges_skipped_cooldown = 0u32;
        let mut ranges_reset = 0u32;

        let min_epoch_hours = if priority == 2 { 12 } else { 56 };
        let now = Utc::now();

        // Load known servers ONCE for all ranges (avoid repeated lock acquisitions)
        let known = self.known_servers.lock().await.clone();

        for range in ranges {
            let network: Ipv4Network = match range.cidr.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let total_ips = network.size() as i64;
            let current_offset = range.scan_offset;

            // If range is exhausted, check epoch cooldown
            if current_offset >= total_ips {
                let can_reset = range.last_scanned_at.map_or(true, |last_scan| {
                    if last_scan.and_utc() > now {
                        true
                    } else {
                        let hours_since = (now - last_scan.and_utc()).num_hours();
                        hours_since >= min_epoch_hours as i64
                    }
                });

                if can_reset {
                    updates.push((range.cidr.clone(), 0_i64, true, true));
                    ranges_reset += 1;
                } else {
                    ranges_skipped_cooldown += 1;
                }
                continue;
            }

            // Compute seed and generate IPs
            let seed = compute_seed(&range.cidr, range.scan_epoch as u64);
            let base_ip = u32::from(network.network());

            let mut added = 0u64;
            let offset = current_offset as u64;

            while (added as usize) < ips_per_range && (offset + added) < total_ips as u64 {
                let position = offset + added;
                let ip_val = ip_at_position(position, base_ip, total_ips as u64, seed);
                let ip_str = format_ip(ip_val);

                let java_key = format!("{}:25565", ip_str);
                let bedrock_key = format!("{}:19132", ip_str);
                let java_known = known.contains(&java_key);
                let bedrock_known = known.contains(&bedrock_key);

                if !java_known {
                    let mut java_target =
                        ServerTarget::new(ip_str.clone(), 25565, "java".to_string());
                    java_target.category = if priority == 2 {
                        AsnCategory::Hosting
                    } else {
                        AsnCategory::Residential
                    };
                    java_target.is_discovery = true;
                    all_targets.push(java_target);
                }

                if !bedrock_known {
                    let mut bedrock_target =
                        ServerTarget::new(ip_str.clone(), 19132, "bedrock".to_string());
                    bedrock_target.category = if priority == 2 {
                        AsnCategory::Hosting
                    } else {
                        AsnCategory::Residential
                    };
                    bedrock_target.is_discovery = true;
                    all_targets.push(bedrock_target);
                }

                added += 1;
            }

            if added == 0 {
                updates.push((range.cidr.clone(), 0_i64, true, true));
                ranges_reset += 1;
                continue;
            }

            let new_offset = offset + added;
            let is_done = new_offset >= total_ips as u64;
            updates.push((
                range.cidr.clone(),
                if is_done { 0 } else { new_offset as i64 },
                is_done,
                is_done,
            ));

            if is_done {
                ranges_reset += 1;
            }
        }

        // Batch update progress in DB
        if !updates.is_empty() {
            if let Err(e) = self.asn_repo.update_batch_range_progress(updates).await {
                tracing::error!("Failed to batch update range progress: {}", e);
            }
        }

        if ranges_skipped_cooldown > 0 || ranges_reset > 0 {
            tracing::info!(
                "Discovery: {} ranges reset, {} ranges skipped cooldown (category: {}, min interval: {}h)",
                ranges_reset,
                ranges_skipped_cooldown,
                if priority == 2 {
                    "hosting"
                } else {
                    "residential"
                },
                min_epoch_hours
            );
        }

        // Shuffle and batch-add to queue
        let generated_count = all_targets.len();
        if !all_targets.is_empty() {
            // Shuffle using a deterministic seed to avoid Send issues with thread_rng
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            now.hash(&mut hasher);
            let seed = hasher.finish();

            // Fisher-Yates shuffle with deterministic RNG
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            all_targets.shuffle(&mut rng);

            self.add_servers_batch(all_targets).await;
        }

        generated_count
    }

    /// Re-queue a server after scanning.
    /// Sets next_scan_at based on priority and whether it was online.
    /// For online servers, also probes adjacent ports (progressive port scanning).
    pub async fn requeue_server(&self, mut server: ServerTarget, was_online: bool) {
        let is_new_discovery = server.last_scanned.is_none();
        let now = Utc::now();
        server.last_scanned = Some(now);

        if was_online {
            server.mark_online();

            // Register as known server to skip in future discovery
            if is_new_discovery {
                self.register_known_server(&server.ip, server.port).await;
            }

            // Progressive port scanning: probe adjacent ports for active servers.
            // Multiple Minecraft servers can run on the same IP with different ports.
            let category = server.category.clone();
            self.probe_adjacent_ports(&server.ip, server.port, category)
                .await;
        } else {
            server.mark_offline();
        }

        // Don't re-queue offline discovery targets (prevents memory bloat)
        if is_new_discovery && !was_online {
            return;
        }

        // Determine priority based on scan result
        if was_online {
            server.priority = 1; // Hot — rescan frequently
        } else if server.consecutive_failures > 5 {
            server.priority = 3; // Cold — scan rarely
        } else {
            server.priority = 2; // Warm
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
        self.add_server(server).await;
    }

    /// Probe adjacent ports (+1, -1) when an online server is found.
    /// Only probes ports that aren't already known servers, and within valid port range.
    /// Probed targets are added to the queue with high priority but a short delay.
    async fn probe_adjacent_ports(&self, ip: &str, base_port: u16, category: AsnCategory) {
        // Only probe if we're not already scanning too many targets for this IP
        // (prevent queue explosion for IPs with many ports)
        let known = self.known_servers.lock().await;

        let mut probe_ports = Vec::with_capacity(2);

        // Probe port +1
        if base_port < 65535 {
            let port = base_port + 1;
            let key = format!("{}:{}", ip, port);
            if !known.contains(&key) {
                probe_ports.push(port);
            }
        }

        // Probe port -1
        if base_port > 1 {
            let port = base_port - 1;
            let key = format!("{}:{}", ip, port);
            if !known.contains(&key) {
                probe_ports.push(port);
            }
        }

        drop(known);

        for port in probe_ports {
            let server_type = if port == 19132 {
                "bedrock".to_string()
            } else {
                "java".to_string()
            };
            let mut target = ServerTarget::new(ip.to_string(), port, server_type);
            target.priority = 1;
            target.category = category.clone();
            target.is_discovery = true;
            // Add a short delay so the main server scan finishes first
            target.next_scan_at = Some(Utc::now() + chrono::Duration::seconds(30));
            self.add_server(target).await;
        }
    }

    /// Get queue statistics for monitoring.
    pub async fn get_queue_stats(&self) -> QueueStats {
        let q = self.queue.lock().await;
        let now = Utc::now();
        let total = q.len();
        let ready = q
            .iter()
            .filter(|s| s.next_scan_at.map_or(true, |t| t <= now))
            .count();
        let discovery_pending = self.discovery_dedup.lock().await.len();

        QueueStats {
            discovery: discovery_pending,
            total,
            ready,
        }
    }

    /// Get queue sizes in the old format for backwards compatibility.
    pub async fn get_queue_readiness(
        &self,
    ) -> ((usize, usize), (usize, usize), (usize, usize), usize) {
        let q = self.queue.lock().await;
        let total = q.len();
        let now = Utc::now();
        let ready = q
            .iter()
            .filter(|s| s.next_scan_at.map_or(true, |t| t <= now))
            .count();
        let discovery_pending = self.discovery_dedup.lock().await.len();

        // Return in format expected by existing code: ((hot_r, hot_t), (warm_r, warm_t), (cold_r, cold_t), discovery)
        // Since we have a unified queue, we report ready/total across all, and discovery_pending separately
        ((ready, total), (0, 0), (0, 0), discovery_pending)
    }

    pub async fn get_discovery_dedup_count(&self) -> usize {
        self.discovery_dedup.lock().await.len()
    }

    /// Periodic background refill: recycles known servers whose scan interval elapsed.
    /// This is a SECONDARY source — discovery generates new IPs, this re-scans known ones.
    pub async fn try_refill_queues(&self) {
        // 1. Recycle dead servers (go to cold queue with 7-day delay)
        if let Ok(dead_servers) = self.server_repo.get_dead_servers(1000).await {
            for server in dead_servers {
                let mut target = ServerTarget::new(
                    server.ip.to_string(),
                    server.port as i32 as u16,
                    server.server_type,
                );
                target.priority = 3;
                if let Some(last) = server.last_seen {
                    target.next_scan_at = Some(last.and_utc() + chrono::Duration::days(7));
                }
                self.add_server(target).await;
            }
        }

        // 2. Refill known servers whose scan interval has elapsed.
        //    Fetch servers from DB that are past their next_scan_at,
        //    limited to avoid flooding the queue.
        if let Ok(servers) = self.server_repo.get_servers_for_refill(2, 24, 5000).await {
            let count = servers.len();
            for server in servers {
                let mut target = ServerTarget::new(
                    server.ip.to_string(),
                    server.port as i32 as u16,
                    server.server_type,
                );
                // Set priority based on the server's state
                if server.consecutive_failures > 5 {
                    target.priority = 3;
                } else if server.consecutive_failures > 0 {
                    target.priority = 2;
                } else {
                    target.priority = 1;
                }
                // Don't set next_scan_at — it's ready now
                self.add_server(target).await;
            }
            tracing::debug!("Refilled {} known servers from DB", count);
        }
    }

    /// Legacy load from DB — kept for backwards compatibility.
    /// Now just used to load known servers at startup.
    pub async fn load_from_database(&self) -> Result<(), sea_orm::DbErr> {
        // Don't load all servers into memory — just ensure known_servers is populated.
        // The scheduler starts empty and fills via discovery + refill.
        Ok(())
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

    let mut a = mix_bits(seed ^ 0x9E3779B97F4A7C15) % range_size;
    if a % 2 == 0 {
        a ^= 1;
    }
    while gcd(a, range_size) != 1 {
        a = (a + 1) % range_size;
        if a == 0 {
            a = 1;
        }
    }
    let b = mix_bits(seed ^ 0x517CC1B727220A95) % range_size;

    let permuted = (a * position + b) % range_size;
    base_ip + permuted as u32
}

#[inline]
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

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

fn compute_seed(cidr: &str, epoch: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cidr.hash(&mut hasher);
    epoch.hash(&mut hasher);
    hasher.finish()
}

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
        let base = 0x0A000000u32;
        let size = 256u64;
        let seed = compute_seed("10.0.0.0/24", 0);

        let mut seen = std::collections::HashSet::new();
        for pos in 0..size {
            let ip = ip_at_position(pos, base, size, seed);
            assert!(ip >= base && ip < base + size as u32);
            assert!(seen.insert(ip), "Duplicate at pos {}", pos);
        }
        assert_eq!(seen.len(), size as usize);
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
    fn test_format_ip() {
        assert_eq!(format_ip(0x0A000001), "10.0.0.1");
        assert_eq!(format_ip(0xC0A80001), "192.168.0.1");
    }
}
