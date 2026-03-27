//! ASN Fetcher service using local MaxMind databases.
//!
//! Downloads and maintains GeoLite2-ASN and GeoLite2-Country databases
//! for fast, local, and limit-free IP categorization.

use crate::models::asn::{AsnCategory, AsnError, AsnManager, AsnRecord};
use crate::repositories::asns::AsnRepository;
use sea_orm::{DatabaseConnection, EntityTrait, Set, TransactionTrait};
use crate::models::entities::{asns, asn_ranges};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time;
use tracing;
use std::path::Path;
use std::fs;
use std::io::Read;
use maxminddb::Reader;
use flate2::read::GzDecoder;

/// Path where MaxMind databases are stored.
const DB_DIR: &str = "data/maxmind";
const ASN_DB_PATH: &str = "data/maxmind/GeoLite2-ASN.mmdb";
const COUNTRY_DB_PATH: &str = "data/maxmind/GeoLite2-Country.mmdb";

/// URL for full ASN database from iptoasn.com
const FULL_ASN_URL: &str = "https://iptoasn.com/data/ip2asn-v4.tsv.gz";

/// URLs for downloading the databases (using a public mirror or requires license key).
/// Note: Standard GeoLite2 requires a license key now.
/// We'll use a placeholder URL and provide instructions.
const ASN_DB_URL: &str = "https://raw.githubusercontent.com/P3TERX/GeoLite.mmdb/download/GeoLite2-ASN.mmdb";
const COUNTRY_DB_URL: &str = "https://raw.githubusercontent.com/P3TERX/GeoLite.mmdb/download/GeoLite2-Country.mmdb";

/// ASN Fetcher using local MaxMind databases.
pub struct AsnFetcher {
    /// HTTP client for downloading updates.
    client: reqwest::Client,
    /// Database connection.
    db: Arc<DatabaseConnection>,
    /// ASN repository.
    asn_repo: Arc<AsnRepository>,
    /// In-memory ASN manager.
    asn_manager: Arc<RwLock<AsnManager>>,
    /// Local MaxMind readers.
    maxmind_asn: Arc<RwLock<Option<Reader<Vec<u8>>>>>,
    maxmind_country: Arc<RwLock<Option<Reader<Vec<u8>>>>>,
}

impl AsnFetcher {
    /// Create a new ASN fetcher.
    pub fn new(db: Arc<DatabaseConnection>, asn_repo: Arc<AsnRepository>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(time::Duration::from_secs(30))
            .user_agent("NMCScan/1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            db,
            asn_repo,
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
        let asns_models = self.asn_repo.get_all_asns().await.unwrap_or_default();
        let ranges_models = self.asn_repo.get_all_asn_ranges().await.unwrap_or_default();

        let mut manager = self.asn_manager.write().await;
        for asn_model in asns_models {
            let category = match asn_model.category.as_str() {
                "hosting" => AsnCategory::Hosting,
                "residential" => AsnCategory::Residential,
                "excluded" => AsnCategory::Excluded,
                _ => AsnCategory::Unknown,
            };
            let tags = asn_model.tags.clone().unwrap_or_default().split(',').map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();
            manager.add_asn(AsnRecord {
                asn: asn_model.asn,
                org: asn_model.org,
                category,
                country: asn_model.country,
                last_updated: asn_model.last_updated.map(|dt| dt.into()),
                server_count: 0,
                tags,
            });
        }
        for range in ranges_models {
            manager.add_range(range.cidr, range.asn);
        }

        Ok(())
    }

    /// Import the full ASN database from iptoasn.com to discover all providers globally.
    pub async fn import_full_database(&self) -> Result<(), AsnError> {
        tracing::info!("Downloading full ASN database from iptoasn.com...");
        let response = self.client.get(FULL_ASN_URL).send().await?;
        if !response.status().is_success() {
            return Err(AsnError::MaxMindError(format!("Failed to download ASN DB: {}", response.status())));
        }

        let bytes = response.bytes().await?;
        let decoder = GzDecoder::new(&bytes[..]);
        let mut tsv_content = String::new();
        let mut gz_reader = decoder;
        gz_reader.read_to_string(&mut tsv_content).map_err(|e| AsnError::MaxMindError(format!("Gzip error: {}", e)))?;

        tracing::info!("Parsing global ASN data ({} entries)...", tsv_content.lines().count());
        
        // Group entries by ASN to reduce categorization calls
        let mut asn_map: std::collections::HashMap<String, (String, String, Vec<String>)> = std::collections::HashMap::new();
        let mut range_dedup: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        for line in tsv_content.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 5 { continue; }

            let range_start = parts[0];
            let range_end = parts[1];
            let asn = format!("AS{}", parts[2]);
            let country = parts[3];
            let org = parts[4];

            if !asn_map.contains_key(&asn) {
                asn_map.insert(asn.clone(), (org.to_string(), country.to_string(), Vec::new()));
            }
            
            // Try to convert range to CIDR
            if let (Ok(start), Ok(end)) = (range_start.parse::<std::net::Ipv4Addr>(), range_end.parse::<std::net::Ipv4Addr>()) {
                let start_octets = start.octets();
                let end_octets = end.octets();
                
                let cidr = format!("{}.{}.{}.0/24", start_octets[0], start_octets[1], start_octets[2]);
                range_dedup.insert(cidr, asn.clone());

                if start_octets[1] != end_octets[1] || (end_octets[2] as i16 - start_octets[2] as i16) > 10 {
                    let mid_cidr = format!("{}.{}.{}.0/24", start_octets[0], start_octets[1], start_octets[2] + 5);
                    range_dedup.insert(mid_cidr, asn);
                }
            }
        }

        let ranges: Vec<(String, String)> = range_dedup.into_iter().collect();
        tracing::info!("Importing {} ASNs and {} discovery ranges into database...", asn_map.len(), ranges.len());
        
        let tx = self.db.begin().await.map_err(|e| AsnError::MaxMindError(format!("Transaction start error: {}", e)))?;

        // Prepare ASN models with categorization
        let mut asn_models: Vec<asns::ActiveModel> = Vec::with_capacity(asn_map.len());
        for (asn, (org, country, _)) in asn_map {
            let (category, tags) = AsnManager::categorize_by_org(&org);
            let tags_str = tags.join(",");
            let cat_str = match category {
                AsnCategory::Hosting => "hosting",
                AsnCategory::Residential => "residential",
                AsnCategory::Excluded => "excluded",
                AsnCategory::Unknown => "unknown",
            }.to_string();
            
            asn_models.push(asns::ActiveModel {
                asn: Set(asn),
                org: Set(org),
                category: Set(cat_str),
                country: Set(Some(country)),
                tags: Set(Some(tags_str)),
                last_updated: Set(Some(Utc::now().into())),
            });
        }

        // Batch upsert ASNs
        for chunk in asn_models.chunks(200) {
            asns::Entity::insert_many(chunk.to_vec())
                .on_conflict(
                    sea_orm::sea_query::OnConflict::column(asns::Column::Asn)
                        .update_columns([asns::Column::Org, asns::Column::Category])
                        .value(asns::Column::Country, sea_orm::sea_query::Expr::cust("COALESCE(excluded.country, asns.country)"))
                        .value(asns::Column::Tags, sea_orm::sea_query::Expr::cust("COALESCE(excluded.tags, asns.tags)"))
                        .value(asns::Column::LastUpdated, sea_orm::sea_query::Expr::cust("CURRENT_TIMESTAMP"))
                        .to_owned()
                )
                .exec(&tx).await
                .map_err(|e| AsnError::MaxMindError(format!("ASN batch error: {}", e)))?;
        }

        // Prepare range models
        let mut range_models: Vec<asn_ranges::ActiveModel> = Vec::with_capacity(ranges.len());
        for (cidr, asn) in ranges {
            range_models.push(asn_ranges::ActiveModel {
                cidr: Set(cidr),
                asn: Set(asn),
                ..Default::default()
            });
        }

        // Batch upsert ranges
        for chunk in range_models.chunks(500) {
            asn_ranges::Entity::insert_many(chunk.to_vec())
                .on_conflict(
                    sea_orm::sea_query::OnConflict::column(asn_ranges::Column::Cidr)
                        .update_column(asn_ranges::Column::Asn)
                        .to_owned()
                )
                .exec(&tx).await
                .map_err(|e| AsnError::MaxMindError(format!("Range batch error: {}", e)))?;
        }

        tx.commit().await.map_err(|e| AsnError::MaxMindError(format!("Transaction commit error: {}", e)))?;

        tracing::info!("Global ASN discovery sync complete.");
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
        let (category, tags) = AsnManager::categorize_by_org(&org);

        let record = AsnRecord {
            asn: asn.clone(),
            org: org.clone(),
            category,
            country: country_code,
            last_updated: Some(Utc::now()),
            server_count: 0,
            tags: tags.clone(),
        };

        // Save to cache for fast dashboard listing
        let _ = self.asn_repo.upsert_asn(
            &asn,
            &org,
            match record.category {
                AsnCategory::Hosting => "hosting",
                AsnCategory::Residential => "residential",
                AsnCategory::Excluded => "excluded",
                AsnCategory::Unknown => "unknown",
            },
            record.country.as_deref(),
            Some(tags),
        ).await;

        // Add range to manager if it's IPv4. 
        // Since lookup_prefix is missing in this context, we jumpstart by adding a /24 range.
        if let std::net::IpAddr::V4(v4) = ip_addr {
            let octets = v4.octets();
            let cidr = format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]);
            let mut manager = self.asn_manager.write().await;
            manager.add_range(cidr.clone(), asn.clone());
            // Also save range to DB
            let _ = self.asn_repo.upsert_asn_range(&cidr, &asn).await;
        }

        // Add to in-memory manager
        let mut manager = self.asn_manager.write().await;
        manager.add_asn(record.clone());

        Ok(record)
    }

    /// Background task to periodically update databases.
    pub async fn run_background_refresh(self: Arc<Self>) {
        tracing::info!("Starting ASN intelligence background update task");

        let mut interval = time::interval(time::Duration::from_secs(86400 * 7)); // Weekly

        loop {
            interval.tick().await;
            tracing::info!("Performing weekly ASN intelligence sync...");
            
            let mut updated = false;
            // 1. Update MaxMind
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

            // 2. Refresh Full ASN Discovery Database
            let _ = self.import_full_database().await;
        }
    }
}
