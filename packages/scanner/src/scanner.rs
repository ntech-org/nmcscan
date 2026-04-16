//! Rate-limited concurrent scanner.
//!
//! Multi-pass scanning architecture:
//! - Pass 1: TCP Connect scan (verify port is open)
//! - Pass 2: Full SLP/RakNet ping (get server status)
//!
//! Hardcoded limits:
//! - Max 200 simultaneous tasks
//! - ~100 new connections per second
//! - 3 second timeout per connection

use crate::syn_scanner;
use nmcscan_shared::utils::exclude::ExcludeManager;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{self, Duration};
use tracing;

use nmcscan_shared::services::asn_fetcher::AsnFetcher;

/// Scanner with rate limiting and concurrency control.
pub struct Scanner {
    semaphore: Arc<Semaphore>,
    rate_limiter: Arc<RateLimiter>,
    cold_rate_limiter: Arc<RateLimiter>,
    tcp_rate_limiter: Option<Arc<RateLimiter>>, // None = no limit for TCP
    exclude_list: Arc<ExcludeManager>,
    asn_fetcher: Arc<AsnFetcher>,
    syn_timeout_ms: u64,
}

/// Token bucket rate limiter.
struct RateLimiter {
    semaphore: Arc<Semaphore>,
    target_rps: u64,
}

impl RateLimiter {
    fn new(rps: u64) -> Arc<Self> {
        let semaphore = Arc::new(Semaphore::new(rps as usize));
        let limiter = Arc::new(Self {
            semaphore: Arc::clone(&semaphore),
            target_rps: rps,
        });

        // Background task to refill tokens frequently for smooth flow
        tokio::spawn(async move {
            // Refill every 10ms for very smooth distribution
            let mut interval = time::interval(Duration::from_millis(10));
            let rps_per_tick = (rps as f64 / 100.0).max(1.0);
            let mut fractional_permits = 0.0;

            loop {
                interval.tick().await;

                fractional_permits += rps_per_tick;
                let to_add = fractional_permits.floor() as usize;

                if to_add > 0 {
                    let current_permits = semaphore.available_permits();
                    // Don't burst more than 1 second worth of permits
                    if current_permits < rps as usize {
                        let actual_add = std::cmp::min(to_add, rps as usize - current_permits);
                        if actual_add > 0 {
                            semaphore.add_permits(actual_add);
                        }
                    }
                    fractional_permits -= to_add as f64;
                }
            }
        });

        limiter
    }

    async fn acquire(&self) {
        // If target RPS is 0, something is wrong, allow but log
        if self.target_rps == 0 {
            return;
        }
        if let Ok(permit) = self.semaphore.acquire().await {
            permit.forget();
        }
    }
}

impl Scanner {
    pub fn new(
        exclude_list: Arc<ExcludeManager>,
        asn_fetcher: Arc<AsnFetcher>,
        rps: u64,
        concurrency: u32,
        cold_rps: Option<u64>,
        tcp_rps: u64,
    ) -> Self {
        let cold_rps = cold_rps.unwrap_or_else(|| (rps / 10).max(1));
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency as usize)),
            rate_limiter: RateLimiter::new(rps),
            cold_rate_limiter: RateLimiter::new(cold_rps),
            tcp_rate_limiter: Some(RateLimiter::new(tcp_rps)), // Separate TCP rate limit
            exclude_list,
            asn_fetcher,
            syn_timeout_ms: 5000,
        }
    }

    /// Pass 1: TCP Connect scan - fast port verification.
/// Uses higher RPS since TCP connect is cheap.
    pub async fn scan_tcp(
        &self,
        ip: &str,
        port: u16,
    ) -> bool {
        let ip_addr: IpAddr = match ip.parse() {
            Ok(addr) => addr,
            Err(_) => return false,
        };

        if self.exclude_list.is_excluded(ip_addr).await {
            return false;
        }

        // TCP has no rate limit - just concurrency control
        if let Some(ref limiter) = self.tcp_rate_limiter {
            limiter.acquire().await;
        }

        let _permit = match self.semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return false,
        };

        let result = syn_scanner::scan_tcp_connect(ip, port, self.syn_timeout_ms).await;
        result.reachable
    }

    /// Pass 2: Full SLP/RakNet scan - get complete server status.
    /// This is the expensive scan, only done after TCP connect passes.
    ///
    /// # Safety
    /// - Checks exclude list BEFORE any connection
    /// - If excluded, SKIP immediately (no log, no ping)
    pub async fn scan_slp(
        &self,
        ip: &str,
        port: u16,
        hostname: Option<&str>,
        priority: i32,
        _is_discovery: bool,
        server_type: &str,
    ) -> Option<nmcscan_shared::network::ScanResult> {
        // Parse IP
        let ip_addr: IpAddr = match ip.parse() {
            Ok(addr) => addr,
            Err(_) => return None,
        };

        // CRITICAL SAFETY CHECK: Exclude list enforcement
        if self.exclude_list.is_excluded(ip_addr).await {
            tracing::debug!("Skipping excluded IP: {}", ip);
            return None;
        }

        // Apply tiered rate limiting
        // Priority 3 (Cold) is limited to 10 RPS, others (Hot/Warm) get full speed
        if priority >= 3 {
            self.cold_rate_limiter.acquire().await;
        } else {
            self.rate_limiter.acquire().await;
        }

        // Acquire concurrency permit
        let _permit = match self.semaphore.acquire().await {
            Ok(p) => p,
            Err(_) => return None,
        };

        // Perform the ping
        let addr = SocketAddr::new(ip_addr, port);

        let ping_result = if server_type == "bedrock" {
            nmcscan_shared::network::raknet::ping_server(addr)
                .await
                .map_err(|e| e.to_string())
        } else {
            nmcscan_shared::network::slp::ping_server(addr, hostname)
                .await
                .map_err(|e| e.to_string())
        };

        // DATA ENRICHMENT: Fetch ASN info
        let mut asn_record = {
            let asn_manager = self.asn_fetcher.asn_manager();
            let manager = asn_manager.read().await;
            if let IpAddr::V4(v4) = ip_addr {
                manager.get_asn_for_ip(v4).cloned()
            } else {
                None
            }
        };

        if asn_record.is_none() {
            if let Ok(record) = self.asn_fetcher.fetch_asn_for_ip(ip).await {
                asn_record = Some(record);
            }
        }

        let (asn, country) = if let Some(record) = asn_record {
            (Some(record.asn), record.country)
        } else {
            (None, None)
        };

        let timestamp = chrono::Utc::now().naive_utc();

        match ping_result {
            Ok(status) => {
                let players_online = status.players.as_ref().map(|p| p.online).unwrap_or(0);
                let players_max = status.players.as_ref().map(|p| p.max).unwrap_or(0);
                let players_sample =
                    status
                        .players
                        .as_ref()
                        .and_then(|p| p.sample.clone())
                        .map(|s| {
                            s.into_iter()
                                .map(|p| nmcscan_shared::network::PlayerSample {
                                    name: p.name,
                                    uuid: p.id,
                                })
                                .collect()
                        });
                let motd = Some(nmcscan_shared::network::slp::extract_motd(&status));
                let version = status.version.as_ref().map(|v| v.name.clone());
                let favicon = status.favicon.clone();
                let brand = Some(nmcscan_shared::network::slp::extract_brand(&status));

                Some(nmcscan_shared::network::ScanResult {
                    ip: ip.to_string(),
                    port,
                    server_type: server_type.to_string(),
                    online: true,
                    players_online,
                    players_max,
                    motd,
                    version,
                    favicon,
                    brand,
                    asn,
                    country: Some(country),
                    players_sample,
                    timestamp,
                })
            }
            Err(_) => Some(nmcscan_shared::network::ScanResult {
                ip: ip.to_string(),
                port,
                server_type: server_type.to_string(),
                online: false,
                players_online: 0,
                players_max: 0,
                motd: None,
                version: None,
                favicon: None,
                brand: None,
                asn,
                country: Some(country),
                players_sample: None,
                timestamp,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "Requires postgres db"]
    async fn test_scanner_build() {
        // Test needs to be updated to use SeaORM and repositories
    }
}
