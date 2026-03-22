//! ASN Fetcher service for dynamically loading ASN data.
//!
//! Fetches ASN information from ipapi.co API and caches in database.
//! Free tier: 1000 requests/day - use sparingly with caching.

use crate::asn::{AsnCategory, AsnError, AsnManager, AsnRecord, IpApiResponse};
use crate::db::Database;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time;
use tracing;

/// Initial IP seeds to kick off ASN discovery.
/// These should be IPs of well-known Minecraft hosting providers.
pub const INITIAL_DISCOVERY_SEEDS: &[&str] = &[
    "176.9.0.1",      // Hetzner
    "51.254.0.1",     // OVH
    "104.248.0.1",    // DigitalOcean
    "45.32.0.1",      // Vultr
    "173.249.0.1",    // Contabo
    "34.64.0.1",      // Google Cloud
    "3.0.0.1",        // AWS
];

/// ASN Fetcher with rate limiting and caching.
pub struct AsnFetcher {
    /// HTTP client for API requests.
    client: reqwest::Client,
    /// Database for caching ASN data.
    db: Arc<Database>,
    /// In-memory ASN manager.
    asn_manager: Arc<RwLock<AsnManager>>,
    /// Rate limiting: requests remaining today.
    requests_remaining: Arc<RwLock<u32>>,
    /// Last rate limit reset time.
    last_reset: Arc<RwLock<chrono::DateTime<Utc>>>,
}

impl AsnFetcher {
    /// Create a new ASN fetcher.
    pub fn new(db: Arc<Database>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(time::Duration::from_secs(10))
            .user_agent("NMCScan/1.0 (Minecraft Server Scanner)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            db,
            asn_manager: Arc::new(RwLock::new(AsnManager::new())),
            requests_remaining: Arc::new(RwLock::new(1000)), // Free tier limit
            last_reset: Arc::new(RwLock::new(Utc::now())),
        }
    }

    /// Get the ASN manager for lookups.
    pub fn asn_manager(&self) -> Arc<RwLock<AsnManager>> {
        Arc::clone(&self.asn_manager)
    }

    /// Initialize ASN data from database and API.
    pub async fn initialize(&self) -> Result<(), AsnError> {
        tracing::info!("Initializing ASN data...");

        // Load from database
        self.load_from_database().await?;

        // Pre-populate database with initial seeds if empty
        if self.db.get_asn_count().await.unwrap_or(0) == 0 {
            self.prepopulate_hosting_asns().await?;
        }

        let manager = self.asn_manager.read().await;
        tracing::info!(
            "ASN initialization complete: {} ASNs, {} ranges",
            manager.asn_count(),
            manager.range_count()
        );

        Ok(())
    }

    /// Load ASN data from database cache.
    async fn load_from_database(&self) -> Result<(), AsnError> {
        let asns: Vec<crate::asn::AsnRecord> = self.db.get_all_asns().await.unwrap_or_default();
        let ranges = self.db.get_all_asn_ranges().await.unwrap_or_default();

        let mut manager = self.asn_manager.write().await;

        for asn in asns {
            manager.add_asn(asn);
        }

        for range in ranges {
            manager.add_range(range.cidr, range.asn);
        }

        tracing::info!(
            "Loaded {} ASNs and {} ranges from database",
            manager.asn_count(),
            manager.range_count()
        );

        Ok(())
    }

    /// Pre-populate the ASN database using initial discovery seeds.
    async fn prepopulate_hosting_asns(&self) -> Result<(), AsnError> {
        tracing::info!("Pre-populating ASN database from initial discovery seeds...");

        for ip in INITIAL_DISCOVERY_SEEDS {
            match self.fetch_asn_for_ip(ip).await {
                Ok(record) => {
                    tracing::info!("Discovered seed ASN: {} ({}) as {:?}", record.asn, record.org, record.category);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch ASN for seed IP {}: {}", ip, e);
                }
            }
        }

        Ok(())
    }

    /// Fetch ASN data for a single IP from API.
    pub async fn fetch_asn_for_ip(&self, ip: &str) -> Result<AsnRecord, AsnError> {
        // Check rate limit
        if !self.check_rate_limit().await {
            return Err(AsnError::AsnNotFound);
        }

        // Check in-memory manager (which includes DB cache)
        if let Ok(ip_addr) = ip.parse::<std::net::Ipv4Addr>() {
            let manager = self.asn_manager.read().await;
            if let Some(cached) = manager.get_asn_for_ip(ip_addr) {
                // Check if cache is still valid (7 days)
                if let Some(last_updated) = cached.last_updated {
                    if Utc::now().signed_duration_since(last_updated) < Duration::days(7) {
                        return Ok(cached.clone());
                    }
                }
            }
        }

        // Fetch from API
        let url = format!("https://ipapi.co/{}/json/", ip);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            tracing::warn!("API request failed for IP {}: {}", ip, response.status());
            return Err(AsnError::AsnNotFound);
        }

        let api_response: IpApiResponse = response.json().await?;

        // Decrement rate limit
        self.decrement_rate_limit().await;

        // Parse response
        let asn = api_response.asn.unwrap_or_else(|| "AS0".to_string());
        let org = api_response.org.unwrap_or_else(|| "Unknown".to_string());
        let category = AsnManager::categorize_by_org(&org);
        let country = api_response.country_code;

        let record = AsnRecord {
            asn: asn.clone(),
            org: org.clone(),
            category,
            country,
            last_updated: Some(Utc::now()),
        };

        // Save to database
        self.db
            .upsert_asn(
                &asn,
                &org,
                match record.category {
                    AsnCategory::Hosting => "hosting",
                    AsnCategory::Residential => "residential",
                    AsnCategory::Excluded => "excluded",
                    AsnCategory::Unknown => "unknown",
                },
                record.country.as_deref(),
            )
            .await
            .unwrap_or_else(|e| tracing::warn!("Failed to save ASN: {}", e));

        // Add to IP ranges if network is provided
        if let Some(network) = api_response.network {
            self.db
                .upsert_asn_range(&network, &asn)
                .await
                .unwrap_or_else(|e| tracing::warn!("Failed to save ASN range: {}", e));

            let mut manager = self.asn_manager.write().await;
            manager.add_range(network, asn);
        }

        // Add to in-memory manager
        {
            let mut manager = self.asn_manager.write().await;
            manager.add_asn(record.clone());
        }

        Ok(record)
    }

    /// Check if we have API requests remaining.
    async fn check_rate_limit(&self) -> bool {
        let remaining = *self.requests_remaining.read().await;
        let last_reset = *self.last_reset.read().await;

        // Reset counter daily
        if Utc::now().signed_duration_since(last_reset) > Duration::days(1) {
            let mut reset = self.last_reset.write().await;
            *reset = Utc::now();
            drop(reset);

            let mut rem = self.requests_remaining.write().await;
            *rem = 1000;
            drop(rem);

            tracing::info!("ASN API rate limit reset");
            return true;
        }

        if remaining > 0 {
            true
        } else {
            tracing::warn!("ASN API rate limit exhausted for today");
            false
        }
    }

    /// Decrement the rate limit counter.
    async fn decrement_rate_limit(&self) {
        let mut remaining = self.requests_remaining.write().await;
        *remaining = remaining.saturating_sub(1);
    }

    /// Get remaining API requests for today.
    pub async fn get_requests_remaining(&self) -> u32 {
        *self.requests_remaining.read().await
    }

    /// Background task to periodically refresh ASN data.
    pub async fn run_background_refresh(self: Arc<Self>) {
        tracing::info!("Starting ASN background refresh task");

        let mut interval = time::interval(time::Duration::from_secs(3600)); // Every hour

        loop {
            interval.tick().await;

            // Refresh stale ASNs (older than 7 days)
            match self.db.get_stale_asns(7).await {
                Ok(stale_asns) => {
                    if stale_asns.is_empty() {
                        continue;
                    }

                    tracing::info!("Refreshing {} stale ASNs", stale_asns.len());

                    // Only refresh a few per hour to stay within rate limit
                    let to_refresh = stale_asns.into_iter().take(10).collect::<Vec<_>>();

                    for _asn in to_refresh {
                        // We'd need an IP from each ASN to refresh - skip for now
                        // This would require tracking sample IPs per ASN
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to get stale ASNs: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_asn_fetcher_creation() {
        let db = Arc::new(Database::new("sqlite::memory:").await.unwrap());
        let fetcher = AsnFetcher::new(Arc::clone(&db));

        assert_eq!(fetcher.get_requests_remaining().await, 1000);
    }
}
