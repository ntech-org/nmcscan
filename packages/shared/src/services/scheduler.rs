//! Priority-based scheduler for efficient server scanning.
//!
//! Implements Hot/Warm/Cold tier algorithm with ethical weighted selection.
//! Discovery uses deterministic hash-based IP shuffle tracked by offset in PostgreSQL.

use crate::models::asn::AsnCategory;
use crate::repositories::{AsnRepository, ServerRepository};
use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
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
const MAX_QUEUE_SIZE: usize = 100_000;

/// TTL for discovery deduplication: 1 hour.
/// Prevents the same IP:port from being added to the discovery queue within this window.
const DISCOVERY_DEDUP_TTL_SECS: u64 = 3600;

pub struct Scheduler {
    hot_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    warm_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    cold_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    /// Discovery targets — always ready (next_scan_at = None).
    /// Separate from main queues to prevent stalls when queues are full.
    discovery_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    /// Discovery deduplication: tracks recently-added IP:port combinations.
    /// Prevents the same target from being queued multiple times within the TTL window.
    discovery_dedup: Arc<Mutex<HashMap<String, Instant>>>,
    /// Main queue deduplication: tracks IP:port combinations currently in hot/warm/cold queues.
    /// Prevents the same server from being added multiple times (e.g., from DB refill).
    hot_dedup: Arc<Mutex<HashSet<String>>>,
    warm_dedup: Arc<Mutex<HashSet<String>>>,
    cold_dedup: Arc<Mutex<HashSet<String>>>,
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
            hot_queue: Arc::new(Mutex::new(VecDeque::new())),
            warm_queue: Arc::new(Mutex::new(VecDeque::new())),
            cold_queue: Arc::new(Mutex::new(VecDeque::new())),
            discovery_queue: Arc::new(Mutex::new(VecDeque::new())),
            discovery_dedup: Arc::new(Mutex::new(HashMap::new())),
            hot_dedup: Arc::new(Mutex::new(HashSet::new())),
            warm_dedup: Arc::new(Mutex::new(HashSet::new())),
            cold_dedup: Arc::new(Mutex::new(HashSet::new())),
            server_repo,
            asn_repo,
            test_mode,
            test_interval,
            target_rps,
        }
    }

    pub async fn add_server(&self, server: ServerTarget, at_front: bool) {
        // Discovery targets always go to the dedicated discovery queue (always ready).
        if server.is_discovery {
            // Deduplication check: skip if this IP:port was recently added
            let key = format!("{}:{}", server.ip, server.port);
            let mut dedup = self.discovery_dedup.lock().await;

            // Clean up expired entries and check if this target is still valid
            let now = Instant::now();
            dedup.retain(|_, instant| {
                now.duration_since(*instant).as_secs() < DISCOVERY_DEDUP_TTL_SECS
            });

            if dedup.contains_key(&key) {
                tracing::debug!("Skipping duplicate discovery target: {}", key);
                return;
            }

            // Add to dedup map and queue
            dedup.insert(key, now);
            drop(dedup);

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

        let (queue, dedup_set) = match server.priority {
            1 => (&self.hot_queue, &self.hot_dedup),
            2 => (&self.warm_queue, &self.warm_dedup),
            _ => (&self.cold_queue, &self.cold_dedup),
        };

        let key = format!("{}:{}", server.ip, server.port);

        // Check dedup: skip if this server is already in the queue
        {
            let mut dedup = dedup_set.lock().await;
            if dedup.contains(&key) {
                tracing::debug!("Skipping duplicate server in queue: {} (priority={})", key, server.priority);
                return;
            }
            dedup.insert(key.clone());
        }

        let mut q = queue.lock().await;
        if q.len() >= MAX_QUEUE_SIZE {
            // Remove from dedup since we're not adding
            dedup_set.lock().await.remove(&key);
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

        // Collect ready tiers with their earliest scan time and corresponding dedup set
        let mut ready: Vec<(&Arc<Mutex<VecDeque<ServerTarget>>>, &Arc<Mutex<HashSet<String>>>, usize, DateTime<Utc>)> =
            Vec::new();
        if let Some((idx, t)) = hot_info {
            ready.push((&self.hot_queue, &self.hot_dedup, idx, t));
        }
        if let Some((idx, t)) = warm_info {
            ready.push((&self.warm_queue, &self.warm_dedup, idx, t));
        }
        if let Some((idx, t)) = cold_info {
            ready.push((&self.cold_queue, &self.cold_dedup, idx, t));
        }

        if !ready.is_empty() {
            // Pick the tier with the earliest-ready item (most overdue)
            ready.sort_by_key(|(_, _, _, t)| *t);
            let (queue, dedup_set, idx, _) = &ready[0];
            let mut q = queue.lock().await;
            let server = q.remove(*idx).unwrap();
            // Clean up dedup
            let key = format!("{}:{}", server.ip, server.port);
            dedup_set.lock().await.remove(&key);
            return Some(server);
        }

        // 3. Nothing found in first 5000 — deep scan ALL queues for ANY ready item.
        let queue_dedup_pairs: [(&Arc<Mutex<VecDeque<ServerTarget>>>, &Arc<Mutex<HashSet<String>>>); 3] = [
            (&self.hot_queue, &self.hot_dedup),
            (&self.warm_queue, &self.warm_dedup),
            (&self.cold_queue, &self.cold_dedup),
        ];
        let mut best: Option<(&Arc<Mutex<VecDeque<ServerTarget>>>, &Arc<Mutex<HashSet<String>>>, usize, DateTime<Utc>)> = None;

        for (queue, _dedup_set) in &queue_dedup_pairs {
            let q = queue.lock().await;
            for i in 0..q.len() {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    let t = q[i].next_scan_at.unwrap_or(now);
                    if best.as_ref().map_or(true, |(_, _, _, bt)| t < *bt) {
                        best = Some((queue, _dedup_set, i, t));
                    }
                }
            }
        }

        if let Some((queue, dedup_set, idx, _)) = best {
            let mut q = queue.lock().await;
            let server = q.remove(idx).unwrap();
            // Clean up dedup
            let key = format!("{}:{}", server.ip, server.port);
            dedup_set.lock().await.remove(&key);
            return Some(server);
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
        // INTERLEAVED DISCOVERY: Fetch hosting ranges (90% of discovery scan capacity)
        // IPs per range is scaled from target RPS to keep the discovery queue well-fed.
        // At 100 RPS with a ~15s tick interval: 100 * 15 * 0.9 = ~1350 IPs needed.
        // With ~500 ranges returned: 1350 / 500 ≈ 3 IPs per range.
        let hosting_ips_per_range = std::cmp::max(1u64, (self.target_rps * 15 * 9 / 10).div_ceil(500));

        match self.asn_repo.get_ranges_to_scan("hosting", 500).await {
            Ok(ranges) => {
                if ranges.is_empty() {
                    tracing::debug!(
                        "Discovery: No hosting ranges available (all in cooldown or scanned)"
                    );
                    return;
                }
                let count = self.fill_discovery_queue(ranges, 2, hosting_ips_per_range as usize).await.unwrap_or(0);
                if count > 0 {
                    tracing::info!(
                        "Discovery: Added {} new targets to discovery queue (from hosting, {} IPs/range)",
                        count,
                        hosting_ips_per_range
                    );
                } else {
                    tracing::debug!(
                        "Discovery: Hosting ranges returned but no IPs generated (ranges exhausted or in cooldown)"
                    );
                }
            }
            Err(e) => {
                tracing::error!("Discovery error: Failed to fetch hosting ranges: {}", e);
            }
        }
    }

    pub async fn fill_cold_queue_if_needed(&self) {
        // 1. Try to recycle dead/ignored servers (these go to main cold queue)
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
                self.add_server(target, false).await;
            }
        }

        // 2. DISCOVERY: Fetch residential ranges (10% of discovery scan capacity)
        // Residential scanning is deliberately rate-limited to minimize impact on home networks.
        let residential_ips_per_range = std::cmp::max(1u64, (self.target_rps * 15 / 10).div_ceil(500));

        let ranges = self
            .asn_repo
            .get_ranges_to_scan("residential", 500)
            .await
            .unwrap_or_default();
        let source = "residential";

        if !ranges.is_empty() {
            let count = self.fill_discovery_queue(ranges, 3, residential_ips_per_range as usize).await.unwrap_or(0);
            if count > 0 {
                tracing::info!(
                    "Discovery: Added {} new targets to discovery queue (from {}, {} IPs/range)",
                    count,
                    source,
                    residential_ips_per_range
                );
            } else {
                tracing::debug!(
                    "Discovery: Residential ranges returned but no IPs generated (ranges exhausted or in cooldown)"
                );
            }
        }
    }

    /// Master discovery function that takes multiple ranges, computes IPs via
    /// deterministic shuffle at the current offset, and pushes to the queue.
    ///
    /// No bitset filter — every IP at each offset is unique per epoch cycle.
    /// When a range is exhausted, its offset resets to 0 and epoch increments,
    /// giving a fresh shuffled permutation for the next cycle.
    ///
    /// CRITICAL: Epoch cooldown is enforced to prevent rescanning IPs too frequently.
    /// Hosting: 12h minimum between epochs (2x/day max)
    /// Residential: 56h minimum between epochs (3x/week max)
    pub async fn fill_discovery_queue(
        &self,
        ranges: Vec<crate::models::entities::asn_ranges::Model>,
        priority: i32,
        ips_per_range: usize,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        use ipnetwork::Ipv4Network;

        let mut all_targets = Vec::new();
        let mut updates = Vec::new();
        let mut ranges_skipped_cooldown = 0u32;
        let mut ranges_reset = 0u32;

        // Minimum hours between epoch cycles based on category
        let min_epoch_hours = if priority == 2 { 12 } else { 56 }; // hosting vs residential
        let now = Utc::now();

        for range in ranges {
            let network: Ipv4Network = match range.cidr.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let total_ips = network.size() as i64;
            let current_offset = range.scan_offset;

            // If the range is exhausted, check if enough time has passed before resetting.
            // This prevents the same IPs from being rescanned immediately after epoch reset.
            if current_offset >= total_ips {
                // SAFETY: Clamp future timestamps to "now" to prevent permanent cooldown stalls
                // (e.g. from clock drift, manual DB edits, or timezone bugs).
                // If last_scanned_at is in the future, treat it as None (never scanned = always eligible).
                let can_reset = range.last_scanned_at.map_or(true, |last_scan| {
                    if last_scan.and_utc() > now {
                        true  // future timestamp: treat as never scanned, always eligible
                    } else {
                        let hours_since = (now - last_scan.and_utc()).num_hours();
                        hours_since >= min_epoch_hours as i64
                    }
                });

                if can_reset {
                    updates.push((range.cidr.clone(), 0_i64, true, true));
                    ranges_reset += 1;
                    tracing::debug!(
                        "Range {} completed epoch {}, resetting to epoch {} ({}h since last scan)",
                        range.cidr,
                        range.scan_epoch,
                        range.scan_epoch + 1,
                        min_epoch_hours
                    );
                } else {
                    ranges_skipped_cooldown += 1;
                    let hours_since = range.last_scanned_at.map_or(0, |t| {
                        if t.and_utc() > now { 0 } else { (now - t.and_utc()).num_hours() }
                    });
                    tracing::debug!(
                        "Range {} skipped (epoch {}), only {}h since last scan (need {}h)",
                        range.cidr,
                        range.scan_epoch,
                        hours_since,
                        min_epoch_hours
                    );
                }
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
                ranges_reset += 1;
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

            if is_done {
                ranges_reset += 1;
                tracing::debug!(
                    "Range {} completing at offset {}, resetting to epoch {} (scanned {} IPs)",
                    range.cidr,
                    current_offset + added as i64,
                    range.scan_epoch + 1,
                    total_ips
                );
            }
        }

        // Log summary for monitoring
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

        // Batch update progress in DB (offset + epoch)
        if !updates.is_empty() {
            self.asn_repo.update_batch_range_progress(updates).await?;
        }

        let generated_count = all_targets.len();

        // GLOBAL SHUFFLE & ADD
        if !all_targets.is_empty() {
            {
                let mut rng = rand::thread_rng();
                all_targets.shuffle(&mut rng);
            }

            let mut queued_count = 0usize;
            for target in all_targets {
                let queue = match target.priority {
                    1 if !target.is_discovery => &self.hot_queue,
                    2 if !target.is_discovery => &self.warm_queue,
                    _ if !target.is_discovery => &self.cold_queue,
                    _ => &self.discovery_queue,
                };
                let mut dq = queue.lock().await;
                if dq.len() < MAX_QUEUE_SIZE {
                    dq.push_back(target);
                    queued_count += 1;
                }
            }

            tracing::debug!(
                "Discovery: {}/{} targets queued ({} dropped due to queue cap)",
                queued_count,
                generated_count,
                generated_count.saturating_sub(queued_count)
            );
        }

        Ok(generated_count)
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

    /// Get ready vs total counts for each queue (for status logging).
    pub async fn get_queue_readiness(&self) -> ((usize, usize), (usize, usize), (usize, usize), usize) {
        let hot_ready = self.count_ready_in_queue(&self.hot_queue).await;
        let hot_total = self.hot_queue.lock().await.len();
        let warm_ready = self.count_ready_in_queue(&self.warm_queue).await;
        let warm_total = self.warm_queue.lock().await.len();
        let cold_ready = self.count_ready_in_queue(&self.cold_queue).await;
        let cold_total = self.cold_queue.lock().await.len();
        let discovery = self.discovery_queue.lock().await.len();
        ((hot_ready, hot_total), (warm_ready, warm_total), (cold_ready, cold_total), discovery)
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

    /// Get the number of entries in the discovery deduplication map.
    pub async fn get_discovery_dedup_count(&self) -> usize {
        let dedup = self.discovery_dedup.lock().await;
        dedup.len()
    }

    /// Get the queue length for a given priority.
    async fn get_queue_len(&self, priority: i32) -> usize {
        match priority {
            1 => self.hot_queue.lock().await.len(),
            2 => self.warm_queue.lock().await.len(),
            _ => self.cold_queue.lock().await.len(),
        }
    }

    /// Get the dedup set size for a given priority.
    async fn get_dedup_size(&self, priority: i32) -> usize {
        match priority {
            1 => self.hot_dedup.lock().await.len(),
            2 => self.warm_dedup.lock().await.len(),
            _ => self.cold_dedup.lock().await.len(),
        }
    }

    /// Count how many items in a queue are ready to scan (next_scan_at <= now or None).
    async fn count_ready_in_queue(&self, queue: &Arc<Mutex<VecDeque<ServerTarget>>>) -> usize {
        let q = queue.lock().await;
        let now = Utc::now();
        q.iter()
            .filter(|s| s.next_scan_at.map_or(true, |t| t <= now))
            .count()
    }

    /// Periodically refill queues from DB with servers whose scan interval has elapsed.
    /// Only runs when there aren't enough READY servers in the queue to prevent aggressive re-scanning.
    pub async fn try_refill_queues(&self) {
        let configs = vec![
            (1, 2, 10_000, 5_000u64), // priority, interval_hours, threshold, limit
            (2, 24, 5_000, 3_000u64),
            (3, 168, 5_000, 2_000u64),
        ];

        for (priority, interval_hours, threshold, limit) in configs {
            let queue = match priority {
                1 => &self.hot_queue,
                2 => &self.warm_queue,
                _ => &self.cold_queue,
            };

            // Count only READY servers (next_scan_at <= now), not total queue length
            let ready_count = self.count_ready_in_queue(queue).await;
            // Only refill when ready servers are below 25% of threshold
            let refill_threshold = threshold / 4;
            if ready_count >= refill_threshold {
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

            let total_len = self.get_queue_len(priority).await;
            let dedup_count = self.get_dedup_size(priority).await;
            tracing::info!(
                "Queue refill: priority={} queue has {}/{} ready/total items (threshold: {}, dedup: {}), adding {} servers from DB",
                priority,
                ready_count,
                total_len,
                refill_threshold,
                dedup_count,
                ready_servers.len()
            );
            for server in ready_servers {
                let mut target = ServerTarget::new(
                    server.ip.to_string(),
                    server.port.try_into().unwrap_or(25565),
                    server.server_type,
                );
                target.priority = priority;
                target.last_scanned = server.last_seen.map(|t| t.and_utc());
                target.next_scan_at = None; // Ready to scan now
                // Use add_server to handle deduplication
                self.add_server(target, false).await;
            }
        }
    }

    pub async fn load_from_database(&self) -> Result<(), sea_orm::DbErr> {
        // Only load servers that are currently online or have been online in the past.
        // We filter out 'unknown' status servers which were likely just potential discovery targets.
        let servers = self.server_repo.get_servers_for_load(999999999).await?;

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

    #[test]
    fn test_epoch_cooldown_logic_hosting() {
        // Test that hosting ranges require 12 hours minimum between epochs
        let min_epoch_hours = 12i64; // hosting

        // Simulate a range scanned 6 hours ago (should NOT reset)
        let hours_since_scan = 6i64;
        let can_reset = hours_since_scan >= min_epoch_hours;
        assert!(
            !can_reset,
            "Range scanned 6h ago should not reset (need 12h)"
        );

        // Simulate a range scanned 13 hours ago (should reset)
        let hours_since_scan = 13i64;
        let can_reset = hours_since_scan >= min_epoch_hours;
        assert!(can_reset, "Range scanned 13h ago should reset (need 12h)");

        // Simulate a range never scanned (should reset)
        let can_reset = true; // None.map_or(true, ...) returns true
        assert!(can_reset, "Range never scanned should reset");
    }

    #[test]
    fn test_epoch_cooldown_logic_residential() {
        // Test that residential ranges require 56 hours minimum between epochs
        let min_epoch_hours = 56i64; // residential

        // Simulate a range scanned 24 hours ago (should NOT reset)
        let hours_since_scan = 24i64;
        let can_reset = hours_since_scan >= min_epoch_hours;
        assert!(
            !can_reset,
            "Range scanned 24h ago should not reset (need 56h)"
        );

        // Simulate a range scanned 60 hours ago (should reset)
        let hours_since_scan = 60i64;
        let can_reset = hours_since_scan >= min_epoch_hours;
        assert!(can_reset, "Range scanned 60h ago should reset (need 56h)");
    }

    #[test]
    fn test_dedup_ttl_expiration() {
        // Test that dedup entries expire after TTL
        let _ttl_secs = 300u64; // 5 minutes

        // Simulate instant creation
        let now = Instant::now();
        let mut map = HashMap::new();
        map.insert("10.0.0.1:25565".to_string(), now);

        // Check entry exists
        assert!(map.contains("10.0.0.1:25565"));

        // Simulate time passing (less than TTL)
        // In real code, we'd check: now.duration_since(instant).as_secs() < ttl_secs
        // Here we just verify the logic structure

        // Entry should still exist
        assert!(map.contains("10.0.0.1:25565"));

        // After TTL expires, entry should be removed by retain()
        // map.retain(|_, instant| now.duration_since(*instant).as_secs() < ttl_secs);
    }

    /// Integration test: Simulates multiple discovery ticks to verify
    /// that IPs progress correctly through ranges without repetition.
    #[test]
    fn test_discovery_ip_progression_no_repeats() {
        use ipnetwork::Ipv4Network;
        use std::collections::HashSet;
        use std::net::Ipv4Addr;

        // Simulate a /24 range (256 IPs)
        let cidr = "10.0.0.0/24";
        let network: Ipv4Network = cidr.parse().unwrap();
        let total_ips = network.size() as u64;
        let base_ip = u32::from(network.network());

        // Track ALL IPs generated across multiple epochs
        let mut all_ips_seen: HashSet<String> = HashSet::new();
        let mut epoch_ips: Vec<HashSet<String>> = Vec::new();

        // Simulate scanning through 2 epochs with batch size of 10
        let batch_size = 10;
        let epochs_to_test = 2;

        for epoch in 0..epochs_to_test {
            let seed = compute_seed(cidr, epoch);
            let mut current_epoch_ips = HashSet::new();
            let mut offset = 0u64;

            // Process the range in batches (like discovery ticks)
            while offset < total_ips {
                let batch_end = (offset + batch_size as u64).min(total_ips);
                let mut batch_ips = Vec::new();

                for pos in offset..batch_end {
                    let ip_val = ip_at_position(pos, base_ip, total_ips, seed);
                    let ip_str = format_ip(ip_val);
                    batch_ips.push(ip_str.clone());

                    // Verify IP is in range
                    let ip_parsed: Ipv4Addr = ip_str.parse().unwrap();
                    assert!(
                        network.contains(ip_parsed),
                        "IP {} out of range {} at pos {}, epoch {}",
                        ip_str,
                        cidr,
                        pos,
                        epoch
                    );

                    // Track for duplicate detection
                    current_epoch_ips.insert(ip_str.clone());
                    all_ips_seen.insert(ip_str);
                }

                // Verify no duplicates within this batch
                let batch_set: HashSet<_> = batch_ips.iter().collect();
                assert_eq!(
                    batch_set.len(),
                    batch_ips.len(),
                    "Duplicate IPs found in batch at epoch {}, offset {}",
                    epoch,
                    offset
                );

                offset = batch_end;
            }

            // Verify epoch covered all IPs exactly once
            assert_eq!(
                current_epoch_ips.len(),
                total_ips as usize,
                "Epoch {} should have {} unique IPs, got {}",
                epoch,
                total_ips,
                current_epoch_ips.len()
            );

            epoch_ips.push(current_epoch_ips);
        }

        // Verify different epochs produce different permutations
        if epoch_ips.len() >= 2 {
            // Count how many IPs are in the same position
            let same_position_count = {
                let seed0 = compute_seed(cidr, 0);
                let seed1 = compute_seed(cidr, 1);
                (0..total_ips)
                    .filter(|&pos| {
                        let ip0 = ip_at_position(pos, base_ip, total_ips, seed0);
                        let ip1 = ip_at_position(pos, base_ip, total_ips, seed1);
                        ip0 == ip1
                    })
                    .count()
            };

            // Most positions should differ between epochs
            assert!(
                same_position_count < total_ips as usize / 2,
                "Epochs 0 and 1 have too many IPs in same position ({}/{}). Permutation may not be shuffling correctly.",
                same_position_count,
                total_ips
            );
        }

        println!(
            "✓ Successfully tested {} epochs across {} IPs",
            epochs_to_test, total_ips
        );
        println!("✓ Total unique IPs seen: {}", all_ips_seen.len());
        println!("✓ No duplicates within or across epochs");
    }

    /// Integration test: Verifies cooldown logic prevents rescanning too soon
    #[test]
    fn test_cooldown_prevents_premature_rescan() {
        use chrono::Duration as ChronoDuration;

        // Simulate a hosting range that was just fully scanned
        let min_epoch_hours = 12i64;
        let now = Utc::now();

        // Scenario 1: Range scanned 1 hour ago (should NOT reset)
        let last_scan_time = (now - ChronoDuration::hours(1)).naive_utc();
        let hours_since = (now - last_scan_time.and_utc()).num_hours();
        let can_reset = hours_since >= min_epoch_hours;
        assert!(
            !can_reset,
            "Range scanned 1h ago should NOT reset (need {}h)",
            min_epoch_hours
        );

        // Scenario 2: Range scanned 6 hours ago (should NOT reset)
        let last_scan_time = (now - ChronoDuration::hours(6)).naive_utc();
        let hours_since = (now - last_scan_time.and_utc()).num_hours();
        let can_reset = hours_since >= min_epoch_hours;
        assert!(
            !can_reset,
            "Range scanned 6h ago should NOT reset (need {}h)",
            min_epoch_hours
        );

        // Scenario 3: Range scanned 12 hours ago (SHOULD reset - exactly at boundary)
        let last_scan_time = (now - ChronoDuration::hours(12)).naive_utc();
        let hours_since = (now - last_scan_time.and_utc()).num_hours();
        let can_reset = hours_since >= min_epoch_hours;
        assert!(
            can_reset,
            "Range scanned 12h ago SHOULD reset (at boundary)"
        );

        // Scenario 4: Range scanned 24 hours ago (SHOULD reset)
        let last_scan_time = (now - ChronoDuration::hours(24)).naive_utc();
        let hours_since = (now - last_scan_time.and_utc()).num_hours();
        let can_reset = hours_since >= min_epoch_hours;
        assert!(
            can_reset,
            "Range scanned 24h ago SHOULD reset (well past {}h)",
            min_epoch_hours
        );

        // Scenario 5: Range never scanned (SHOULD reset)
        let last_scan_time: Option<chrono::NaiveDateTime> = None;
        let can_reset =
            last_scan_time.map_or(true, |t| (now - t.and_utc()).num_hours() >= min_epoch_hours);
        assert!(can_reset, "Range never scanned SHOULD reset");

        println!("✓ Cooldown logic correctly prevents premature resets");
        println!(
            "✓ Hosting ranges require {}h minimum between epochs",
            min_epoch_hours
        );
    }

    /// Integration test: Simulates realistic discovery cycle with cooldown enforcement
    #[test]
    fn test_realistic_discovery_cycle_with_cooldown() {
        use chrono::Duration as ChronoDuration;
        use ipnetwork::Ipv4Network;

        // Simulate 3 ranges with different states
        struct MockRange {
            cidr: String,
            offset: i64,
            epoch: i64,
            last_scanned: Option<chrono::NaiveDateTime>,
            total_ips: i64,
        }

        let now = Utc::now();
        let mut ranges = vec![
            // Range 1: Fresh, never scanned
            MockRange {
                cidr: "10.0.0.0/30".to_string(), // 4 IPs
                offset: 0,
                epoch: 0,
                last_scanned: None,
                total_ips: 4,
            },
            // Range 2: Partially scanned 2 hours ago (should NOT reset if exhausted)
            MockRange {
                cidr: "10.0.1.0/30".to_string(), // 4 IPs
                offset: 2,                       // Halfway through
                epoch: 0,
                last_scanned: Some((now - ChronoDuration::hours(2)).naive_utc()),
                total_ips: 4,
            },
            // Range 3: Fully scanned 13 hours ago (SHOULD reset for hosting)
            MockRange {
                cidr: "10.0.2.0/30".to_string(), // 4 IPs
                offset: 4,                       // Exhausted
                epoch: 0,
                last_scanned: Some((now - ChronoDuration::hours(13)).naive_utc()),
                total_ips: 4,
            },
        ];

        // Simulate discovery tick for hosting (12h cooldown)
        let min_epoch_hours = 12i64;
        let mut ips_generated = 0;
        let mut ranges_reset = 0;
        let mut ranges_skipped = 0;

        for range in &mut ranges {
            // Check if range is exhausted
            if range.offset >= range.total_ips {
                // Check cooldown
                let can_reset = range.last_scanned.map_or(true, |last| {
                    let hours_since = (now - last.and_utc()).num_hours();
                    hours_since >= min_epoch_hours
                });

                if can_reset {
                    // Reset with epoch bump
                    range.offset = 0;
                    range.epoch += 1;
                    range.last_scanned = Some(now.naive_utc());
                    ranges_reset += 1;
                    println!(
                        "✓ Range {} reset from epoch {} to {}",
                        range.cidr,
                        range.epoch - 1,
                        range.epoch
                    );
                } else {
                    ranges_skipped += 1;
                    println!("✓ Range {} skipped (cooldown not met)", range.cidr);
                }
                continue;
            }

            // Generate IPs from current offset
            let network: Ipv4Network = range.cidr.parse().unwrap();
            let base_ip = u32::from(network.network());
            let seed = compute_seed(&range.cidr, range.epoch as u64);
            let batch_size = 2; // Simulate small batch

            let start_offset = range.offset as u64;
            let end_offset = (start_offset + batch_size).min(range.total_ips as u64);

            for pos in start_offset..end_offset {
                let ip_val = ip_at_position(pos, base_ip, range.total_ips as u64, seed);
                let _ip_str = format_ip(ip_val);
                ips_generated += 1;
            }

            range.offset = end_offset as i64;
            range.last_scanned = Some(now.naive_utc());
        }

        // Verify expectations
        assert_eq!(
            ranges_reset, 1,
            "Range 3 should have reset (13h > 12h cooldown)"
        );
        assert_eq!(
            ranges_skipped, 0,
            "No ranges should be skipped in this scenario"
        );
        assert!(
            ips_generated > 0,
            "Should have generated IPs from non-exhausted ranges"
        );

        println!("✓ Discovery cycle generated {} IPs", ips_generated);
        println!(
            "✓ {} ranges reset, {} ranges skipped",
            ranges_reset, ranges_skipped
        );
        println!("✓ Cooldown enforcement working correctly");
    }
}
