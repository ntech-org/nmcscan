//! Priority-based scheduler for efficient server scanning.
//!
//! Implements Hot/Warm/Cold tier algorithm:
//! - **Hot (Tier 1)**: Online servers, last seen < 4 hours - ran 2-4 times/day
//! - **Warm (Tier 2)**: Known hosting ASN ranges, not scanned in 7 days - ran 2-3 times/week
//! - **Cold (Tier 3)**: Residential IPs, high-failure servers - ran 1-2 times/month

use chrono::Utc;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::asn::AsnCategory;
use crate::db::Database;

/// Server target for scanning.
#[derive(Debug, Clone)]
pub struct ServerTarget {
    pub ip: String,
    pub port: u16,
    pub hostname: Option<String>,
    pub priority: i32,
    pub consecutive_failures: i32,
    pub category: AsnCategory,
    pub last_scanned: Option<chrono::DateTime<Utc>>,
    pub next_scan_at: Option<chrono::DateTime<Utc>>,
    pub scan_count: u32,
    pub success_rate: f32,
}

impl ServerTarget {
    pub fn new(ip: String, port: u16) -> Self {
        Self {
            ip,
            port,
            hostname: None,
            priority: 2, // Default to Warm
            consecutive_failures: 0,
            category: AsnCategory::Unknown,
            last_scanned: None,
            next_scan_at: None,
            scan_count: 0,
            success_rate: 0.5,
        }
    }

    /// Mark server as online: reset failures, set priority to Hot.
    pub fn mark_online(&mut self) {
        self.consecutive_failures = 0;
        self.priority = 1; // Hot
        self.success_rate = ((self.success_rate * self.scan_count as f32) + 1.0)
            / (self.scan_count + 1) as f32;
        self.scan_count += 1;
    }

    /// Mark server as offline: increment failures, potentially demote to Cold.
    pub fn mark_offline(&mut self) {
        self.consecutive_failures += 1;
        self.success_rate = ((self.success_rate * self.scan_count as f32))
            / (self.scan_count + 1) as f32;
        self.scan_count += 1;

        if self.consecutive_failures > 5 {
            self.priority = 3; // Cold
        }
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip.parse().unwrap(), self.port)
    }

    /// Calculate weighted priority for scheduling.
    /// Lower score = higher priority.
    pub fn scheduling_score(&self) -> f32 {
        let base_priority = self.priority as f32;
        let time_factor = if let Some(last) = self.last_scanned {
            let hours_since = Utc::now()
                .signed_duration_since(last)
                .num_hours() as f32;
            // Older scans get lower score (higher priority)
            1.0 / (1.0 + hours_factor(hours_since))
        } else {
            1.0 // Never scanned = high priority
        };

        let success_factor = if self.success_rate > 0.5 {
            0.8 // Successful servers scanned more often
        } else {
            1.2 // Less successful servers scanned less often
        };

        base_priority * time_factor * success_factor
    }
}

/// Factor for time-based priority decay.
fn hours_factor(hours: f32) -> f32 {
    // Exponential decay: priority increases with time
    (hours / 4.0).exp() - 1.0
}

/// Known hosting ASN ranges for Warm tier targeting.
pub const HOSTING_ASN_RANGES: &[(&str, &str)] = &[
    ("5.9.0.0/16", "AS24940"),     // Hetzner
    ("46.4.0.0/16", "AS24940"),    // Hetzner
    ("78.46.0.0/15", "AS24940"),   // Hetzner
    ("88.198.0.0/16", "AS24940"),  // Hetzner
    ("116.202.0.0/16", "AS24940"), // Hetzner
    ("135.181.0.0/16", "AS24940"), // Hetzner
    ("138.201.0.0/16", "AS24940"), // Hetzner
    ("142.132.0.0/16", "AS24940"), // Hetzner
    ("144.76.0.0/16", "AS24940"),  // Hetzner
    ("148.251.0.0/16", "AS24940"), // Hetzner
    ("157.90.0.0/16", "AS24940"),  // Hetzner
    ("159.69.0.0/16", "AS24940"),  // Hetzner
    ("162.55.0.0/16", "AS24940"),  // Hetzner
    ("167.233.0.0/16", "AS24940"), // Hetzner
    ("168.119.0.0/16", "AS24940"), // Hetzner
    ("176.9.0.0/16", "AS24940"),   // Hetzner
    ("188.40.0.0/16", "AS24940"),  // Hetzner
    ("195.201.0.0/16", "AS24940"), // Hetzner
    ("213.133.96.0/19", "AS24940"),// Hetzner
    ("104.16.0.0/12", "AS13335"),  // Cloudflare
    ("172.64.0.0/13", "AS13335"),  // Cloudflare
    ("35.192.0.0/12", "AS15169"),  // GCP
    ("34.64.0.0/10", "AS15169"),   // GCP
    ("52.0.0.0/6", "AS16509"),     // AWS
    ("54.0.0.0/8", "AS16509"),     // AWS
    ("13.32.0.0/15", "AS16509"),   // AWS
    ("20.0.0.0/4", "AS8075"),      // Azure
    ("40.64.0.0/10", "AS8075"),    // Azure
    ("64.225.0.0/16", "AS14061"),  // DigitalOcean
    ("104.131.0.0/16", "AS14061"), // DigitalOcean
    ("138.197.0.0/16", "AS14061"), // DigitalOcean
    ("139.59.0.0/16", "AS14061"),  // DigitalOcean
    ("157.230.0.0/16", "AS14061"), // DigitalOcean
    ("159.65.0.0/16", "AS14061"),  // DigitalOcean
    ("165.22.0.0/16", "AS14061"),  // DigitalOcean
    ("167.99.0.0/16", "AS14061"),  // DigitalOcean
    ("178.62.0.0/16", "AS14061"),  // DigitalOcean
    ("45.32.0.0/16", "AS20473"),   // Vultr
    ("45.76.0.0/16", "AS20473"),   // Vultr
    ("108.61.0.0/16", "AS20473"),  // Vultr
    ("149.28.0.0/16", "AS20473"),  // Vultr
    ("207.148.0.0/20", "AS20473"), // Vultr
    ("51.38.0.0/16", "AS16276"),   // OVH
    ("51.77.0.0/16", "AS16276"),   // OVH
    ("51.89.0.0/16", "AS16276"),   // OVH
    ("135.125.0.0/16", "AS16276"), // OVH
    ("141.94.0.0/16", "AS16276"),  // OVH
    ("146.59.0.0/16", "AS16276"),  // OVH
    ("151.80.0.0/16", "AS16276"),  // OVH
    ("158.69.0.0/16", "AS16276"),  // OVH
    ("164.132.0.0/16", "AS16276"), // OVH
    ("167.114.0.0/16", "AS16276"), // OVH
    ("176.31.0.0/16", "AS16276"),  // OVH
    ("178.32.0.0/15", "AS16276"),  // OVH
    ("188.165.0.0/16", "AS16276"), // OVH
    ("192.95.0.0/16", "AS16276"),  // OVH
    ("192.99.0.0/16", "AS16276"),  // OVH
];

/// Priority scheduler managing Hot/Warm/Cold queues.
pub struct Scheduler {
    hot_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    warm_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    cold_queue: Arc<Mutex<VecDeque<ServerTarget>>>,
    /// Database for loading servers and ASN data.
    db: Arc<Database>,
    /// Track ASN scan frequency for smart Warm targeting.
    asn_scan_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    /// Track ASN last scan time for rotation.
    asn_last_scanned: Arc<Mutex<std::collections::HashMap<String, chrono::DateTime<Utc>>>>,
    /// Test mode configuration
    pub test_mode: bool,
    pub test_interval: u64,
}

impl Scheduler {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            hot_queue: Arc::new(Mutex::new(VecDeque::new())),
            warm_queue: Arc::new(Mutex::new(VecDeque::new())),
            cold_queue: Arc::new(Mutex::new(VecDeque::new())),
            db,
            asn_scan_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
            asn_last_scanned: Arc::new(Mutex::new(std::collections::HashMap::new())),
            test_mode: false,
            test_interval: 60,
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
    /// This now skips servers that are not yet ready for their next scan.
    pub async fn next_server(&self) -> Option<ServerTarget> {
        let now = Utc::now();

        // Try Hot queue first
        {
            let mut q = self.hot_queue.lock().await;
            for i in 0..q.len() {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    return q.remove(i);
                }
                // Only check first few to avoid O(N) in hot loop
                if i >= 10 { break; }
            }
        }

        // Then Warm queue
        {
            let mut q = self.warm_queue.lock().await;
            for i in 0..q.len() {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    return q.remove(i);
                }
                if i >= 10 { break; }
            }
        }

        // Finally Cold queue
        {
            let mut q = self.cold_queue.lock().await;
            for i in 0..q.len() {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    return q.remove(i);
                }
                if i >= 10 { break; }
            }
        }

        None
    }

    /// Dynamic range filling: adds new servers from hosting ASNs if Warm queue is low.
    pub async fn fill_warm_queue_if_needed(&self) {
        let warm_len = self.warm_queue.lock().await.len();
        if warm_len > 500 {
            return; // Already has enough work
        }

        if let Some(asn) = self.select_next_asn_for_warm_scan().await {
            tracing::info!("Dynamic filling: Selecting ASN {} for discovery", asn);
            
            // Find CIDRs for this ASN
            let ranges = self.db.get_all_asn_ranges().await.unwrap_or_default();
            let asn_ranges: Vec<_> = ranges.into_iter().filter(|r| r.asn == asn).collect();
            
            if let Some(range) = asn_ranges.first() {
                if let Err(e) = self.add_asn_range_servers(&range.cidr, &asn).await {
                    tracing::warn!("Failed to add servers from range {}: {}", range.cidr, e);
                }
                self.record_asn_scan(&asn).await;
            }
        }
    }

    /// Re-queue a server after scanning with updated status.
    pub async fn requeue_server(&self, mut server: ServerTarget, was_online: bool) {
        let now = Utc::now();
        server.last_scanned = Some(now);

        if was_online {
            server.mark_online();
        } else {
            server.mark_offline();
        }

        // Calculate next scan time
        let delay = if self.test_mode {
            chrono::Duration::seconds(self.test_interval as i64)
        } else {
            match server.priority {
                1 => chrono::Duration::hours(4),   // Hot: every 4 hours
                2 => chrono::Duration::hours(24),  // Warm: every 24 hours
                _ => chrono::Duration::days(7),    // Cold: every week
            }
        };
        server.next_scan_at = Some(now + delay);

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

    /// Get detailed queue statistics.
    pub async fn get_queue_stats(&self) -> SchedulerStats {
        let hot = self.hot_queue.lock().await;
        let warm = self.warm_queue.lock().await;
        let cold = self.cold_queue.lock().await;

        let hot_hosting = hot.iter().filter(|s| s.category == AsnCategory::Hosting).count();
        let warm_hosting = warm.iter().filter(|s| s.category == AsnCategory::Hosting).count();
        let cold_residential = cold.iter().filter(|s| s.category == AsnCategory::Residential).count();

        SchedulerStats {
            hot_total: hot.len(),
            hot_hosting,
            warm_total: warm.len(),
            warm_hosting,
            cold_total: cold.len(),
            cold_residential,
        }
    }

    /// Load servers from database and populate queues.
    pub async fn load_from_database(&self) -> Result<(), crate::db::DatabaseError> {
        let servers = self.db.get_all_servers(None, 10000).await?;

        for server in servers {
            let mut target = ServerTarget::new(server.ip, server.port as u16);
            target.priority = server.priority;
            target.consecutive_failures = server.consecutive_failures;

            // Determine category based on IP (would need ASN lookup)
            // For now, default to Unknown
            target.category = AsnCategory::Unknown;

            if let Some(last_seen) = server.last_seen {
                let last = last_seen.and_utc();
                target.last_scanned = Some(last);
                
                // Calculate when it should have been scanned next
                let delay = match target.priority {
                    1 => chrono::Duration::hours(4),
                    2 => chrono::Duration::hours(24),
                    _ => chrono::Duration::days(7),
                };
                target.next_scan_at = Some(last + delay);
            }

            self.add_server(target).await;
        }

        Ok(())
    }

    /// Add servers from a specific ASN range for Warm scanning.
    pub async fn add_asn_range_servers(
        &self,
        cidr: &str,
        asn: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use ipnetwork::Ipv4Network;

        let network: Ipv4Network = cidr.parse()?;
        let mut count = 0;

        // Sample IPs from the range (don't add all, just a representative sample)
        let sample_size = std::cmp::min(network.size() as u32, 100);
        let step = std::cmp::max(1, network.size() as u32 / sample_size);

        for (i, ip) in network.iter().enumerate() {
            if i % step as usize != 0 {
                continue;
            }

            // Skip network and broadcast addresses
            if i == 0 || i == network.size() as usize - 1 {
                continue;
            }

            let ip_str = ip.to_string();
            let mut target = ServerTarget::new(ip_str.clone(), 25565);
            target.category = AsnCategory::Hosting;
            target.priority = 2; // Warm

            let _ = self.db.insert_server_if_new(&ip_str, 25565).await;

            self.add_server(target).await;
            count += 1;

            // Limit to 50 servers per range
            if count >= 50 {
                break;
            }
        }

        // Update ASN scan tracking
        {
            let mut counts = self.asn_scan_counts.lock().await;
            *counts.entry(asn.to_string()).or_insert(0) += 1;
        }
        {
            let mut scanned = self.asn_last_scanned.lock().await;
            *scanned.entry(asn.to_string()).or_insert(Utc::now()) = Utc::now();
        }

        tracing::debug!("Added {} servers from ASN range {} ({})", count, cidr, asn);

        Ok(())
    }

    /// Smart ASN selection for Warm scans.
    /// Prioritizes ASNs with:
    /// - High historical server density
    /// - Low recent scan frequency
    /// - Good success rates
    pub async fn select_next_asn_for_warm_scan(&self) -> Option<String> {
        let counts = self.asn_scan_counts.lock().await;
        let last_scanned = self.asn_last_scanned.lock().await;

        let mut best_asn: Option<String> = None;
        let mut best_score = f32::MIN;

        for (cidr, asn) in HOSTING_ASN_RANGES {
            let scan_count = counts.get(*asn).copied().unwrap_or(0) as f32;
            let hours_since = last_scanned
                .get(*asn)
                .map(|last| {
                    Utc::now()
                        .signed_duration_since(*last)
                        .num_hours() as f32
                })
                .unwrap_or(168.0); // Default to 1 week if never scanned

            // Score: prefer ASNs not recently scanned
            let time_score = hours_score(hours_since);
            // Penalize over-scanned ASNs
            let frequency_penalty = (scan_count / 10.0).min(2.0);

            let score = time_score - frequency_penalty;

            if score > best_score {
                best_score = score;
                best_asn = Some(asn.to_string());
            }
        }

        best_asn
    }

    /// Record an ASN scan for smart rotation.
    pub async fn record_asn_scan(&self, asn: &str) {
        let mut counts = self.asn_scan_counts.lock().await;
        *counts.entry(asn.to_string()).or_insert(0) += 1;

        let mut scanned = self.asn_last_scanned.lock().await;
        *scanned.entry(asn.to_string()).or_insert(Utc::now()) = Utc::now();
    }

    /// Get scan statistics for an ASN.
    pub async fn get_asn_scan_stats(&self, asn: &str) -> AsnScanStats {
        let counts = self.asn_scan_counts.lock().await;
        let last_scanned = self.asn_last_scanned.lock().await;

        let count = counts.get(asn).copied().unwrap_or(0);
        let last = last_scanned.get(asn).copied();
        let hours_since = last
            .map(|l| Utc::now().signed_duration_since(l).num_hours())
            .unwrap_or(-1);

        AsnScanStats {
            asn: asn.to_string(),
            scan_count: count,
            last_scanned: last,
            hours_since_scan: if hours_since < 0 { None } else { Some(hours_since as f32) },
        }
    }
}

/// Calculate time-based score for ASN selection.
fn hours_score(hours: f32) -> f32 {
    // Higher score for ASNs not scanned recently
    // Caps at 168 hours (1 week)
    (hours / 24.0).min(7.0)
}

/// Statistics about the scheduler queues.
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub hot_total: usize,
    pub hot_hosting: usize,
    pub warm_total: usize,
    pub warm_hosting: usize,
    pub cold_total: usize,
    pub cold_residential: usize,
}

/// Statistics about ASN scanning.
#[derive(Debug, Clone)]
pub struct AsnScanStats {
    pub asn: String,
    pub scan_count: u32,
    pub last_scanned: Option<chrono::DateTime<Utc>>,
    pub hours_since_scan: Option<f32>,
}

impl Default for Scheduler {
    fn default() -> Self {
        // Requires database - use Scheduler::new instead
        panic!("Scheduler::default() not available, use Scheduler::new(db)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_target_online() {
        let mut server = ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 3,
            consecutive_failures: 5,
            category: AsnCategory::Unknown,
            last_scanned: None,
            scan_count: 0,
            success_rate: 0.5,
        };

        server.mark_online();
        assert_eq!(server.priority, 1);
        assert_eq!(server.consecutive_failures, 0);
        assert_eq!(server.scan_count, 1);
    }

    #[test]
    fn test_server_target_offline() {
        let mut server = ServerTarget {
            ip: "192.0.2.1".to_string(),
            port: 25565,
            priority: 2,
            consecutive_failures: 5,
            category: AsnCategory::Unknown,
            last_scanned: None,
            scan_count: 0,
            success_rate: 0.5,
        };

        server.mark_offline();
        assert_eq!(server.consecutive_failures, 6);
        assert_eq!(server.priority, 3); // Should become cold after >5 failures
    }

    #[test]
    fn test_hours_factor() {
        assert!(hours_factor(0.0) < hours_factor(4.0));
        assert!(hours_factor(4.0) < hours_factor(24.0));
    }

    #[test]
    fn test_hours_score() {
        // Score should increase with time, capped at 7
        assert!(hours_score(0.0) < hours_score(24.0));
        assert!(hours_score(24.0) < hours_score(168.0));
        assert_eq!(hours_score(168.0), 7.0); // Cap
        assert_eq!(hours_score(500.0), 7.0); // Still capped
    }

    #[test]
    fn test_hosting_asn_ranges() {
        assert!(!HOSTING_ASN_RANGES.is_empty());
        for (cidr, asn) in HOSTING_ASN_RANGES {
            assert!(cidr.contains('/'));
            assert!(asn.starts_with("AS"));
        }
    }
}
