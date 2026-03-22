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

        // Pre-populate known hosting ASNs
        self.prepopulate_hosting_asns().await?;

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
        let asns = self.db.get_all_asns().await.unwrap_or_default();
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

    /// Pre-populate known hosting provider ASNs and their ranges.
    async fn prepopulate_hosting_asns(&self) -> Result<(), AsnError> {
        tracing::info!("Pre-populating known hosting ranges...");

        let mut manager = self.asn_manager.write().await;

        for (cidr, asn, org) in KNOWN_HOSTING_RANGES {
            // 1. Add/Update ASN record
            let record = AsnRecord {
                asn: asn.to_string(),
                org: org.to_string(),
                category: AsnCategory::Hosting,
                country: None, // Could be improved if needed
                last_updated: Some(Utc::now()),
            };
            manager.add_asn(record);

            // Save ASN to database
            if let Err(e) = self.db.upsert_asn(asn, org, "hosting", None).await {
                tracing::warn!("Failed to save ASN {}: {}", asn, e);
            }

            // 2. Add/Update ASN range
            manager.add_range(cidr.to_string(), asn.to_string());
            
            // Save range to database
            if let Err(e) = self.db.upsert_asn_range(cidr, asn).await {
                tracing::warn!("Failed to save ASN range {}: {}", cidr, e);
            }
        }

        tracing::info!("Pre-populated {} hosting provider ranges", KNOWN_HOSTING_RANGES.len());

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

/// Known hosting ASN ranges for Warm scan targeting.
/// These are pre-populated CIDR ranges for major hosting providers.
pub const KNOWN_HOSTING_RANGES: &[(&str, &str, &str)] = &[
    // AWS
    ("3.0.0.0/15", "AS16509", "AMAZON-02"),
    ("13.32.0.0/15", "AS16509", "AMAZON-02"),
    ("52.0.0.0/11", "AS16509", "AMAZON-02"),
    ("54.0.0.0/11", "AS16509", "AMAZON-02"),
    // Google Cloud
    ("34.64.0.0/10", "AS15169", "GOOGLE"),
    ("35.184.0.0/13", "AS15169", "GOOGLE"),
    // Azure
    ("20.0.0.0/11", "AS8075", "MICROSOFT-CORP"),
    ("40.64.0.0/10", "AS8075", "MICROSOFT-CORP"),
    // Hetzner
    ("5.9.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("46.4.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("78.46.0.0/15", "AS24940", "HETZNER-ONLINE"),
    ("88.198.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("116.202.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("135.181.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("138.201.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("142.132.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("144.76.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("148.251.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("157.90.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("159.69.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("162.55.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("167.233.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("168.119.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("176.9.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("188.40.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("195.201.0.0/16", "AS24940", "HETZNER-ONLINE"),
    ("213.133.96.0/19", "AS24940", "HETZNER-ONLINE"),
    // OVH
    ("51.38.0.0/16", "AS16276", "OVH"),
    ("51.68.0.0/16", "AS16276", "OVH"),
    ("51.75.0.0/16", "AS16276", "OVH"),
    ("51.77.0.0/16", "AS16276", "OVH"),
    ("51.79.0.0/16", "AS16276", "OVH"),
    ("51.81.0.0/16", "AS16276", "OVH"),
    ("51.83.0.0/16", "AS16276", "OVH"),
    ("51.89.0.0/16", "AS16276", "OVH"),
    ("51.91.0.0/16", "AS16276", "OVH"),
    ("135.125.0.0/16", "AS16276", "OVH"),
    ("137.74.0.0/16", "AS16276", "OVH"),
    ("141.94.0.0/16", "AS16276", "OVH"),
    ("141.95.0.0/16", "AS16276", "OVH"),
    ("144.2.0.0/16", "AS16276", "OVH"),
    ("145.239.0.0/16", "AS16276", "OVH"),
    ("146.59.0.0/16", "AS16276", "OVH"),
    ("147.135.0.0/16", "AS16276", "OVH"),
    ("151.80.0.0/16", "AS16276", "OVH"),
    ("152.228.0.0/16", "AS16276", "OVH"),
    ("158.69.0.0/16", "AS16276", "OVH"),
    ("164.132.0.0/16", "AS16276", "OVH"),
    ("167.114.0.0/16", "AS16276", "OVH"),
    ("176.31.0.0/16", "AS16276", "OVH"),
    ("178.32.0.0/15", "AS16276", "OVH"),
    ("185.15.68.0/22", "AS16276", "OVH"),
    ("188.165.0.0/16", "AS16276", "OVH"),
    ("192.95.0.0/16", "AS16276", "OVH"),
    ("192.99.0.0/16", "AS16276", "OVH"),
    ("193.70.0.0/17", "AS16276", "OVH"),
    ("198.27.64.0/18", "AS16276", "OVH"),
    ("198.50.128.0/17", "AS16276", "OVH"),
    ("199.231.0.0/16", "AS16276", "OVH"),
    ("213.186.32.0/19", "AS16276", "OVH"),
    ("213.251.128.0/18", "AS16276", "OVH"),
    // DigitalOcean
    ("64.225.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("68.183.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("104.131.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("104.236.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("104.248.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("107.170.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("128.199.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("134.209.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("138.197.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("138.68.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("139.59.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("142.93.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("143.110.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("143.198.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("146.190.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("147.182.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("157.230.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("157.245.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("159.65.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("159.89.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("161.35.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("162.243.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("164.90.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("165.22.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("165.227.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("167.71.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("167.99.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("167.172.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("170.64.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("174.138.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("178.62.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("178.128.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("188.166.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("188.226.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("192.241.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("194.116.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("206.189.0.0/16", "AS14061", "DIGITALOCEAN-ASN"),
    ("209.97.128.0/18", "AS14061", "DIGITALOCEAN-ASN"),
    ("2400:6180::/32", "AS14061", "DIGITALOCEAN-ASN"),
    // Cloudflare
    ("104.16.0.0/12", "AS13335", "CLOUDFLARENET"),
    ("172.64.0.0/13", "AS13335", "CLOUDFLARENET"),
    // Vultr/Choopa
    ("45.32.0.0/16", "AS20473", "AS-CHOOPA"),
    ("45.63.0.0/16", "AS20473", "AS-CHOOPA"),
    ("45.76.0.0/16", "AS20473", "AS-CHOOPA"),
    ("45.77.0.0/16", "AS20473", "AS-CHOOPA"),
    ("63.209.32.0/19", "AS20473", "AS-CHOOPA"),
    ("66.42.0.0/16", "AS20473", "AS-CHOOPA"),
    ("95.179.128.0/17", "AS20473", "AS-CHOOPA"),
    ("108.61.0.0/16", "AS20473", "AS-CHOOPA"),
    ("140.82.0.0/16", "AS20473", "AS-CHOOPA"),
    ("144.202.0.0/16", "AS20473", "AS-CHOOPA"),
    ("149.28.0.0/16", "AS20473", "AS-CHOOPA"),
    ("155.138.128.0/17", "AS20473", "AS-CHOOPA"),
    ("194.68.44.0/24", "AS20473", "AS-CHOOPA"),
    ("199.247.0.0/16", "AS20473", "AS-CHOOPA"),
    ("207.148.0.0/20", "AS20473", "AS-CHOOPA"),
    ("207.246.64.0/19", "AS20473", "AS-CHOOPA"),
    ("208.167.224.0/19", "AS20473", "AS-CHOOPA"),
    ("216.128.128.0/17", "AS20473", "AS-CHOOPA"),
    // Contabo
    ("173.249.0.0/16", "AS51167", "CONTABO"),
    ("176.9.0.0/16", "AS51167", "CONTABO"),
    ("188.138.0.0/16", "AS51167", "CONTABO"),
    ("207.180.192.0/18", "AS51167", "CONTABO"),
    ("212.133.0.0/16", "AS51167", "CONTABO"),
];

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

    #[tokio::test]
    async fn test_known_hosting_ranges() {
        assert!(!KNOWN_HOSTING_RANGES.is_empty());
        for (cidr, asn, org) in KNOWN_HOSTING_RANGES {
            assert!(cidr.contains('/'));
            assert!(asn.starts_with("AS"));
            assert!(!org.is_empty());
        }
    }
}
