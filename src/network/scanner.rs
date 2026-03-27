//! Rate-limited concurrent scanner.
//! 
//! Hardcoded limits:
//! - Max 200 simultaneous tasks
//! - ~100 new connections per second
//! - 3 second timeout per connection

use crate::repositories::ServerRepository;
use crate::utils::exclude::ExcludeManager;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{self, Duration};
use tracing;

use crate::services::asn_fetcher::AsnFetcher;

/// Scanner with rate limiting and concurrency control.
pub struct Scanner {
    semaphore: Arc<Semaphore>,
    rate_limiter: Arc<RateLimiter>,
    cold_rate_limiter: Arc<RateLimiter>,
    exclude_list: Arc<ExcludeManager>,
    server_repo: Arc<ServerRepository>,
    asn_fetcher: Arc<AsnFetcher>,
}

/// Simple but efficient token bucket rate limiter using a Semaphore.
struct RateLimiter {
    semaphore: Semaphore,
}

impl RateLimiter {
    fn new(rps: u64) -> Arc<Self> {
        let limiter = Arc::new(Self {
            semaphore: Semaphore::new(rps as usize),
        });

        // Background task to refill tokens frequently for smooth flow
        let limiter_clone = Arc::clone(&limiter);
        tokio::spawn(async move {
            // Refill every 50ms for smoother distribution
            let mut interval = time::interval(Duration::from_millis(50));
            let rps_per_tick = (rps as f64 / 20.0).max(1.0) as usize;
            loop {
                interval.tick().await;
                let current_permits = limiter_clone.semaphore.available_permits();
                if current_permits < rps as usize {
                    let to_add = std::cmp::min(rps_per_tick, rps as usize - current_permits);
                    if to_add > 0 {
                        limiter_clone.semaphore.add_permits(to_add);
                    }
                }
            }
        });

        limiter
    }

    async fn acquire(&self) {
        if let Ok(permit) = self.semaphore.acquire().await {
            permit.forget();
        }
    }
}

impl Scanner {
    pub fn new(
        exclude_list: Arc<ExcludeManager>,
        server_repo: Arc<ServerRepository>,
        asn_fetcher: Arc<AsnFetcher>,
        rps: u64,
        concurrency: u32,
        cold_rps: Option<u64>,
    ) -> Self {
        let cold_rps = cold_rps.unwrap_or_else(|| (rps / 10).max(1));
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency as usize)),
            rate_limiter: RateLimiter::new(rps),
            cold_rate_limiter: RateLimiter::new(cold_rps),
            exclude_list,
            server_repo,
            asn_fetcher,
        }
    }

    /// Scan a single server with safety checks.
    /// 
    /// # Safety
    /// - Checks exclude list BEFORE any connection
    /// - If excluded, SKIP immediately (no log, no ping)
    pub async fn scan_server(&self, ip: &str, port: u16, hostname: Option<&str>, priority: i32, _is_discovery: bool, server_type: &str) -> (bool, bool) {
        // Parse IP
        let ip_addr: IpAddr = match ip.parse() {
            Ok(addr) => addr,
            Err(_) => return (false, false),
        };

        // CRITICAL SAFETY CHECK: Exclude list enforcement
        if self.exclude_list.is_excluded(ip_addr).await {
            tracing::debug!("Skipping excluded IP: {}", ip);
            return (false, false);
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
            Err(_) => return (false, false),
        };

        // Perform the ping
        let addr = SocketAddr::new(ip_addr, port);
        
        let ping_result = if server_type == "bedrock" {
            crate::network::raknet::ping_server(addr).await.map_err(|e| e.to_string())
        } else {
            crate::network::slp::ping_server(addr, hostname).await.map_err(|e| e.to_string())
        };
        
        match ping_result {
            Ok(status) => {
                // Server is online
                let players_online = status.players.as_ref().map(|p| p.online).unwrap_or(0);
                let players_max = status.players.as_ref().map(|p| p.max).unwrap_or(0);
                let players_sample = status.players.as_ref().and_then(|p| p.sample.clone());
                let motd = Some(crate::network::slp::extract_motd(&status));
                let version = status.version.as_ref().map(|v| v.name.clone());
                let favicon = status.favicon.clone();
                let brand = Some(crate::network::slp::extract_brand(&status));

                // DATA ENRICHMENT: If we don't have ASN info for this IP, fetch it now
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
                    tracing::debug!("Fetching missing ASN info for discovery: {}", ip);
                    if let Ok(record) = self.asn_fetcher.fetch_asn_for_ip(ip).await {
                        asn_record = Some(record);
                    }
                }

                let (asn, country) = if let Some(record) = asn_record {
                    (Some(record.asn), record.country)
                } else {
                    (None, None)
                };

                let is_new = match self.server_repo.mark_online(ip, port as i32, server_type, players_online, players_max, motd, version, players_sample, favicon, brand, asn, country).await {
                    Ok(new) => new,
                    Err(e) => {
                        tracing::error!("Failed to update DB for {}:{}: {}", ip, port, e);
                        false
                    }
                };
                tracing::info!("Server {}:{} is online ({} players)", ip, port, players_online);
                (true, is_new)
            }
            Err(e) => {
                // Server is offline or unreachable
                tracing::debug!("Server {}:{} is offline: {}", ip, port, e);
                
                // DATA ENRICHMENT: Even for offline servers, we want to know their ASN for categorization
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

                if let Err(e) = self.server_repo.mark_offline(ip, port as i32, server_type, asn, country).await {
                    tracing::error!("Failed to update DB for {}:{}: {}", ip, port, e);
                }
                
                (false, false)
            }
        }
    }
    }


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    #[ignore = "Requires postgres db"]
    async fn test_scanner_build() {
        // Test needs to be updated to use SeaORM and repositories
    }
}
