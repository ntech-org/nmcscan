//! ASN Fetcher service using local MaxMind databases.
//!
//! Downloads and maintains GeoLite2-ASN and GeoLite2-Country databases
//! for fast, local, and limit-free IP categorization.

use crate::asn::{AsnCategory, AsnError, AsnManager, AsnRecord};
use crate::db::Database;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time;
use tracing;
use std::path::Path;
use std::fs;
use maxminddb::Reader;

/// Path where MaxMind databases are stored.
const DB_DIR: &str = "data/maxmind";
const ASN_DB_PATH: &str = "data/maxmind/GeoLite2-ASN.mmdb";
const COUNTRY_DB_PATH: &str = "data/maxmind/GeoLite2-Country.mmdb";

/// URLs for downloading the databases (using a public mirror or requires license key).
/// Note: Standard GeoLite2 requires a license key now.
/// We'll use a placeholder URL and provide instructions.
const ASN_DB_URL: &str = "https://raw.githubusercontent.com/P3TERX/GeoLite.mmdb/download/GeoLite2-ASN.mmdb";
const COUNTRY_DB_URL: &str = "https://raw.githubusercontent.com/P3TERX/GeoLite.mmdb/download/GeoLite2-Country.mmdb";

/// ASN Fetcher using local MaxMind databases.
pub struct AsnFetcher {
    /// HTTP client for downloading updates.
    client: reqwest::Client,
    /// Database for caching extra metadata.
    db: Arc<Database>,
    /// In-memory ASN manager.
    asn_manager: Arc<RwLock<AsnManager>>,
    /// Local MaxMind readers.
    maxmind_asn: Arc<RwLock<Option<Reader<Vec<u8>>>>>,
    maxmind_country: Arc<RwLock<Option<Reader<Vec<u8>>>>>,
}

impl AsnFetcher {
    /// Create a new ASN fetcher.
    pub fn new(db: Arc<Database>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(time::Duration::from_secs(30))
            .user_agent("NMCScan/1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            db,
            asn_manager: Arc::new(RwLock::new(AsnManager::new())),
            maxmind_asn: Arc::new(RwLock::new(None)),
            maxmind_country: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the ASN manager for lookups.
    pub fn asn_manager(&self) -> Arc<RwLock<AsnManager>> {
        Arc::clone(&self.asn_manager)
    }

    /// Initialize ASN data and MaxMind databases.
    pub async fn initialize(&self) -> Result<(), AsnError> {
        tracing::info!("Initializing ASN fetcher with MaxMind databases...");

        // Ensure directory exists
        if let Err(e) = fs::create_dir_all(DB_DIR) {
            tracing::error!("Failed to create MaxMind directory: {}", e);
        }

        // 1. Load or download databases
        self.ensure_databases().await?;

        // 2. Load readers into memory
        self.reload_readers().await?;

        // 3. Load extra ASN mapping from SQLite
        self.load_from_database().await?;

        let manager = self.asn_manager.read().await;
        tracing::info!(
            "ASN initialization complete: {} custom ASNs/ranges cached.",
            manager.asn_count()
        );

        Ok(())
    }

    async fn ensure_databases(&self) -> Result<(), AsnError> {
        if !Path::new(ASN_DB_PATH).exists() {
            tracing::info!("ASN database missing, downloading...");
            self.download_db(ASN_DB_URL, ASN_DB_PATH).await?;
        }

        if !Path::new(COUNTRY_DB_PATH).exists() {
            tracing::info!("Country database missing, downloading...");
            self.download_db(COUNTRY_DB_URL, COUNTRY_DB_PATH).await?;
        }

        Ok(())
    }

    async fn download_db(&self, url: &str, path: &str) -> Result<(), AsnError> {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            tracing::error!("Failed to download MaxMind DB from {}: {}", url, response.status());
            return Err(AsnError::AsnNotFound);
        }

        let bytes = response.bytes().await?;
        fs::write(path, bytes).map_err(|e| {
            tracing::error!("Failed to save MaxMind DB: {}", e);
            AsnError::AsnNotFound
        })?;

        tracing::info!("Successfully downloaded {}", path);
        Ok(())
    }

    async fn reload_readers(&self) -> Result<(), AsnError> {
        if Path::new(ASN_DB_PATH).exists() {
            match Reader::open_readfile(ASN_DB_PATH) {
                Ok(reader) => {
                    let mut lock = self.maxmind_asn.write().await;
                    *lock = Some(reader);
                }
                Err(e) => tracing::error!("Failed to load ASN database: {}", e),
            }
        }

        if Path::new(COUNTRY_DB_PATH).exists() {
            match Reader::open_readfile(COUNTRY_DB_PATH) {
                Ok(reader) => {
                    let mut lock = self.maxmind_country.write().await;
                    *lock = Some(reader);
                }
                Err(e) => tracing::error!("Failed to load Country database: {}", e),
            }
        }

        Ok(())
    }

    /// Load ASN data from database cache (extra mappings not in MaxMind).
    async fn load_from_database(&self) -> Result<(), AsnError> {
        let asns: Vec<crate::asn::AsnRecord> = self.db.get_all_asns().await.unwrap_or_default();
        let ranges = self.db.get_all_asn_ranges().await.unwrap_or_default();

        let mut manager = self.asn_manager.write().await;
        for asn in asns { manager.add_asn(asn); }
        for range in ranges { manager.add_range(range.cidr, range.asn); }

        Ok(())
    }

    /// Fetch ASN and Country data for a single IP using local MaxMind databases.
    pub async fn fetch_asn_for_ip(&self, ip: &str) -> Result<AsnRecord, AsnError> {
        let ip_addr: std::net::IpAddr = ip.parse().map_err(|_| AsnError::AsnNotFound)?;

        // 1. Try local ASN lookup
        let asn_reader_lock = self.maxmind_asn.read().await;
        let mut asn_val = None;
        let mut org_val = None;

        if let Some(reader) = &*asn_reader_lock {
            if let Ok(result) = reader.lookup(ip_addr) {
                if let Ok(Some(asn_db)) = result.decode::<maxminddb::geoip2::Asn>() {
                    asn_val = asn_db.autonomous_system_number.map(|n| format!("AS{}", n));
                    org_val = asn_db.autonomous_system_organization.map(|s: &str| s.to_string());
                }
            }
        }
        drop(asn_reader_lock);

        // 2. Try local Country lookup
        let country_reader_lock = self.maxmind_country.read().await;
        let mut country_code = None;

        if let Some(reader) = &*country_reader_lock {
            if let Ok(result) = reader.lookup(ip_addr) {
                if let Ok(Some(country_db)) = result.decode::<maxminddb::geoip2::Country>() {
                    country_code = country_db.country.iso_code.map(|s: &str| s.to_string());
                }
            }
        }
        drop(country_reader_lock);

        let asn = asn_val.unwrap_or_else(|| "AS0".to_string());
        let org = org_val.unwrap_or_else(|| "Unknown".to_string());
        let category = AsnManager::categorize_by_org(&org);

        let record = AsnRecord {
            asn: asn.clone(),
            org: org.clone(),
            category,
            country: country_code,
            last_updated: Some(Utc::now()),
            server_count: 0,
        };

        // Save to SQLite cache for fast dashboard listing
        let _ = self.db.upsert_asn(
            &asn,
            &org,
            match record.category {
                AsnCategory::Hosting => "hosting",
                AsnCategory::Residential => "residential",
                AsnCategory::Excluded => "excluded",
                AsnCategory::Unknown => "unknown",
            },
            record.country.as_deref(),
        ).await;

        // Add to in-memory manager
        let mut manager = self.asn_manager.write().await;
        manager.add_asn(record.clone());

        Ok(record)
    }

    /// Background task to periodically update databases.
    pub async fn run_background_refresh(self: Arc<Self>) {
        tracing::info!("Starting MaxMind background update task");

        let mut interval = time::interval(time::Duration::from_secs(86400 * 7)); // Weekly

        loop {
            interval.tick().await;
            tracing::info!("Checking for MaxMind database updates...");
            
            let mut updated = false;
            if let Ok(_) = self.download_db(ASN_DB_URL, ASN_DB_PATH).await {
                updated = true;
            }
            if let Ok(_) = self.download_db(COUNTRY_DB_URL, COUNTRY_DB_PATH).await {
                updated = true;
            }

            if updated {
                let _ = self.reload_readers().await;
                tracing::info!("MaxMind databases reloaded.");
            }
        }
    }
}
