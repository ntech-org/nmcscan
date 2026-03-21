//! Priority-based scheduler for efficient server scanning.
//!
//! Implements Hot/Warm/Cold tier algorithm with ethical weighted selection.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use crate::db::Database;
use crate::asn::AsnCategory;
use rand::prelude::*;

/// Pre-defined hosting ASN ranges for discovery.
pub const HOSTING_ASN_RANGES: &[(&str, &str)] = &[
    ("5.9.0.0/16", "AS24940"),    // Hetzner
    ("95.216.0.0/15", "AS24940"), // Hetzner
    ("135.181.0.0/16", "AS24940"),// Hetzner
    ("144.76.0.0/16", "AS24940"), // Hetzner
    ("148.251.0.0/16", "AS24940"),// Hetzner
    ("176.9.0.0/16", "AS24940"),  // Hetzner
    ("188.40.0.0/16", "AS24940"), // Hetzner
    ("51.161.0.0/16", "AS16276"), // OVH
    ("51.178.0.0/16", "AS16276"), // OVH
    ("51.195.0.0/16", "AS16276"), // OVH
    ("51.210.0.0/16", "AS16276"), // OVH
    ("51.222.0.0/16", "AS16276"), // OVH
    ("51.254.0.0/16", "AS16276"), // OVH
    ("54.36.0.0/16", "AS16276"),  // OVH
    ("141.94.0.0/15", "AS16276"), // OVH
    ("142.132.0.0/15", "AS24940"),// Hetzner (New range)
    ("37.187.0.0/16", "AS16276"), // OVH
    ("45.137.204.0/22", "AS212238"), // Datacamp
    ("185.248.140.0/22", "AS212238"),// Datacamp
];

/// A target server for scanning.
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
    pub scan_count: u32,
    pub success_rate: f32,
}

impl ServerTarget {
    pub fn new(ip: String, port: u16) -> Self {
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
    db: Arc<Database>,
    pub test_mode: bool,
    test_interval: u32,
    asn_scan_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    asn_last_scanned: Arc<Mutex<std::collections::HashMap<String, DateTime<Utc>>>>,
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
        }
    }

    pub async fn add_server(&self, server: ServerTarget) {
        let queue = match server.priority {
            1 => &self.hot_queue,
            2 => &self.warm_queue,
            _ => &self.cold_queue,
        };
        queue.lock().await.push_back(server);
    }

    /// Get the next server to scan using weighted random selection to prevent tier starvation.
    pub async fn next_server(&self) -> Option<ServerTarget> {
        let now = Utc::now();
        let roll = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..100)
        };

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
            for i in 0..q.len() {
                if q[i].next_scan_at.map_or(true, |t| t <= now) {
                    return q.remove(i);
                }
                if i >= 10 { break; }
            }
        }
        None
    }

    pub async fn fill_warm_queue_if_needed(&self) {
        let warm_len = self.warm_queue.lock().await.len();
        if warm_len > 1000 { return; }

        if let Some(asn) = self.select_next_asn_for_warm_scan().await {
            tracing::info!("Discovery: Selecting Hosting ASN {} for Warm queue", asn);
            let ranges = self.db.get_all_asn_ranges().await.unwrap_or_default();
            let asn_ranges: Vec<_> = ranges.into_iter().filter(|r| r.asn == asn).collect();
            if let Some(range) = asn_ranges.first() {
                let _ = self.add_shuffled_range(&range.cidr, &asn, 2).await;
                self.record_asn_scan(&asn).await;
            }
        }
    }

    pub async fn fill_cold_queue_if_needed(&self) {
        let cold_len = self.cold_queue.lock().await.len();
        if cold_len > 500 { return; }

        let asns = self.db.get_asns_by_category("residential").await.unwrap_or_default();
        if let Some(asn_record) = asns.first() {
            tracing::info!("Discovery: Selecting Residential ASN {} for Cold queue", asn_record.asn);
            let ranges = self.db.get_all_asn_ranges().await.unwrap_or_default();
            let asn_ranges: Vec<_> = ranges.into_iter().filter(|r| r.asn == asn_record.asn).collect();
            if let Some(range) = asn_ranges.first() {
                let _ = self.add_shuffled_range(&range.cidr, &asn_record.asn, 3).await;
                self.record_asn_scan(&asn_record.asn).await;
            }
        }
    }

    pub async fn add_shuffled_range(
        &self,
        cidr: &str,
        asn: &str,
        priority: i32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use ipnetwork::Ipv4Network;
        use rand::seq::SliceRandom;

        let network: Ipv4Network = cidr.parse()?;
        let mut ips = Vec::new();

        let sample_size = std::cmp::min(network.size() as u32, 100);
        let step = std::cmp::max(1, network.size() as u32 / sample_size);

        for (i, ip) in network.iter().enumerate() {
            if i % step as usize != 0 { continue; }
            if i == 0 || i == network.size() as usize - 1 { continue; }
            ips.push(ip);
            if ips.len() >= 100 { break; }
        }

        {
            let mut rng = rand::thread_rng();
            ips.shuffle(&mut rng);
        }

        let max_to_add = if priority == 2 { 50 } else { 10 };
        let mut count = 0;

        for ip in ips {
            let ip_str = ip.to_string();
            let mut target = ServerTarget::new(ip_str.clone(), 25565);
            target.category = if priority == 2 { AsnCategory::Hosting } else { AsnCategory::Residential };
            target.priority = priority;

            let _ = self.db.insert_server_if_new(&ip_str, 25565).await;
            self.add_server(target).await;
            count += 1;
            if count >= max_to_add { break; }
        }
        Ok(())
    }

    pub async fn requeue_server(&self, mut server: ServerTarget, was_online: bool) {
        let now = Utc::now();
        server.last_scanned = Some(now);

        if was_online {
            server.mark_online();
        } else {
            server.mark_offline();
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
        self.add_server(server).await;
    }

    pub async fn get_queue_sizes(&self) -> (usize, usize, usize) {
        (
            self.hot_queue.lock().await.len(),
            self.warm_queue.lock().await.len(),
            self.cold_queue.lock().await.len(),
        )
    }

    pub async fn load_from_database(&self) -> Result<(), crate::db::DatabaseError> {
        let servers = self.db.get_all_servers(None, None, 10000).await?;
        for server in servers {
            let mut target = ServerTarget::new(server.ip, server.port as u16);
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
            self.add_server(target).await;
        }
        Ok(())
    }

    pub async fn select_next_asn_for_warm_scan(&self) -> Option<String> {
        let counts = self.asn_scan_counts.lock().await;
        let last_scanned = self.asn_last_scanned.lock().await;
        let mut best_asn: Option<String> = None;
        let mut best_score = f32::MIN;

        for (_cidr, asn) in HOSTING_ASN_RANGES {
            let scan_count = counts.get(*asn).copied().unwrap_or(0) as f32;
            let hours_since = last_scanned.get(*asn)
                .map(|last| Utc::now().signed_duration_since(*last).num_hours() as f32)
                .unwrap_or(168.0);

            let time_score = (hours_since / 24.0).min(7.0);
            let frequency_penalty = (scan_count / 10.0).min(2.0);
            let score = time_score - frequency_penalty;

            if score > best_score {
                best_score = score;
                best_asn = Some(asn.to_string());
            }
        }
        best_asn
    }

    pub async fn record_asn_scan(&self, asn: &str) {
        let mut counts = self.asn_scan_counts.lock().await;
        *counts.entry(asn.to_string()).or_insert(0) += 1;
        let mut scanned = self.asn_last_scanned.lock().await;
        *scanned.entry(asn.to_string()).or_insert(Utc::now()) = Utc::now();
    }
}
