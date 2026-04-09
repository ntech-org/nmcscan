//! ASN Fetcher service using local MaxMind databases.
//!
//! Downloads and maintains GeoLite2-ASN and GeoLite2-Country databases
//! for fast, local, and limit-free IP categorization.

use crate::models::asn::{AsnCategory, AsnError, AsnManager, AsnRecord};
use crate::models::entities::{asn_ranges, asns};
use crate::repositories::asns::AsnRepository;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
};
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time;
use tracing;

/// Convert an IP range (inclusive start, inclusive end) to a list of CIDR blocks.
/// Uses greedy largest-block-first algorithm to minimize the number of CIDRs.
fn ip_range_to_cidrs(start: u32, end: u32) -> Vec<String> {
    let mut cidrs = Vec::new();
    let mut current = start;

    while current <= end {
        // Find the largest block size that:
        // 1. Starts at `current` (is aligned)
        // 2. Doesn't exceed `end`
        let mut size = 1u32;
        for bit in (0..32).rev() {
            let block_size = 1u32 << bit;
            // Check alignment: current must be a multiple of block_size
            if current % block_size == 0 && current + block_size - 1 <= end {
                size = block_size;
                break;
            }
        }

        let prefix = 32 - size.trailing_zeros() as u8;
        let ip = Ipv4Addr::from(current);
        cidrs.push(format!("{}/{}", ip, prefix));
        current += size;
    }

    cidrs
}
use flate2::read::GzDecoder;
use maxminddb::Reader;
use serde::Deserialize;
use std::fs;
use std::io::Read;

#[derive(Deserialize)]
pub struct IpverseAsn {
    pub asn: u32,
    pub metadata: Option<IpverseMetadata>,
}

#[derive(Deserialize)]
pub struct IpverseMetadata {
    pub category: Option<String>,
}

/// Path where MaxMind databases are stored.
const DB_DIR: &str = "data/maxmind";
const ASN_DB_PATH: &str = "data/maxmind/GeoLite2-ASN.mmdb";
const COUNTRY_DB_PATH: &str = "data/maxmind/GeoLite2-Country.mmdb";

/// URL for full ASN database from iptoasn.com
const FULL_ASN_URL: &str = "https://iptoasn.com/data/ip2asn-v4.tsv.gz";
const IPVERSE_ASN_URL: &str =
    "https://raw.githubusercontent.com/ipverse/as-metadata/master/as.json";

/// URLs for downloading the databases (using a public mirror or requires license key).
/// Note: Standard GeoLite2 requires a license key now.
/// We'll use a placeholder URL and provide instructions.
const ASN_DB_URL: &str =
    "https://raw.githubusercontent.com/P3TERX/GeoLite.mmdb/download/GeoLite2-ASN.mmdb";
const COUNTRY_DB_URL: &str =
    "https://raw.githubusercontent.com/P3TERX/GeoLite.mmdb/download/GeoLite2-Country.mmdb";

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
            tracing::error!(
                "Failed to download MaxMind DB from {}: {}",
                url,
                response.status()
            );
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
            let tags = asn_model
                .tags
                .clone()
                .unwrap_or_default()
                .split(',')
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect();
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

    pub async fn fetch_ipverse_map(&self) -> std::collections::HashMap<String, String> {
        tracing::info!("Fetching ipverse ASN category map...");
        let mut ipverse_map = std::collections::HashMap::new();

        match self.client.get(IPVERSE_ASN_URL).send().await {
            Ok(ipverse_response) => {
                if ipverse_response.status().is_success() {
                    match ipverse_response.json::<Vec<IpverseAsn>>().await {
                        Ok(ipverse_asns) => {
                            for item in ipverse_asns {
                                if let Some(meta) = item.metadata {
                                    if let Some(cat) = meta.category {
                                        ipverse_map.insert(format!("AS{}", item.asn), cat);
                                    }
                                }
                            }
                            tracing::info!(
                                "Successfully fetched {} ipverse ASN categories",
                                ipverse_map.len()
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse ipverse JSON: {}", e);
                        }
                    }
                } else {
                    tracing::error!(
                        "ipverse API returned non-success status: {}",
                        ipverse_response.status()
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to fetch ipverse data: {}", e);
            }
        }

        ipverse_map
    }

    /// Import the full ASN database from iptoasn.com to discover all providers globally.
    ///
    /// If `clean_slate` is true, all existing ASN data is dropped before importing.
    /// Categorization happens during import using ipverse — no separate recategorization pass needed.
    pub async fn import_full_database(&self, clean_slate: bool) -> Result<(), AsnError> {
        if clean_slate {
            tracing::info!("Dropping existing ASN data for clean import...");
            self.asn_repo
                .drop_all_asn_data()
                .await
                .map_err(|e| AsnError::MaxMindError(format!("Drop error: {}", e)))?;
            // Clear in-memory cache too
            let mut manager = self.asn_manager.write().await;
            *manager = AsnManager::new();
            drop(manager);
        }

        tracing::info!("Downloading ASN categorization metadata from ipverse...");
        let ipverse_map = self.fetch_ipverse_map().await;
        tracing::info!(
            "ipverse map contains {} categorized ASNs",
            ipverse_map.len()
        );

        tracing::info!("Downloading full ASN database from iptoasn.com...");
        let response = self.client.get(FULL_ASN_URL).send().await?;
        if !response.status().is_success() {
            return Err(AsnError::MaxMindError(format!(
                "Failed to download ASN DB: {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;
        let decoder = GzDecoder::new(&bytes[..]);
        let mut tsv_content = String::new();
        let mut gz_reader = decoder;
        gz_reader
            .read_to_string(&mut tsv_content)
            .map_err(|e| AsnError::MaxMindError(format!("Gzip error: {}", e)))?;

        tracing::info!(
            "Parsing global ASN data ({} entries)...",
            tsv_content.lines().count()
        );

        // Group entries by ASN to reduce categorization calls
        let mut asn_map: std::collections::HashMap<String, (String, String, Vec<String>)> =
            std::collections::HashMap::new();
        let mut range_dedup: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for line in tsv_content.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 5 {
                continue;
            }

            let range_start = parts[0];
            let range_end = parts[1];
            let asn = format!("AS{}", parts[2]);
            let country = parts[3];
            let org = parts[4];

            if !asn_map.contains_key(&asn) {
                asn_map.insert(
                    asn.clone(),
                    (org.to_string(), country.to_string(), Vec::new()),
                );
            }

            // Convert IP range to proper CIDR blocks (not just one /24)
            if let (Ok(start), Ok(end)) = (
                range_start.parse::<Ipv4Addr>(),
                range_end.parse::<Ipv4Addr>(),
            ) {
                let start_int = u32::from(start);
                let end_int = u32::from(end);

                for cidr in ip_range_to_cidrs(start_int, end_int) {
                    range_dedup.entry(cidr).or_insert_with(|| asn.clone());
                }
            }
        }

        let ranges: Vec<(String, String)> = range_dedup.into_iter().collect();
        tracing::info!(
            "Importing {} ASNs and {} discovery ranges (throttled)...",
            asn_map.len(),
            ranges.len()
        );

        // Prepare ASN models with categorization
        let mut asn_models: Vec<asns::ActiveModel> = Vec::with_capacity(asn_map.len());
        for (asn, (org, country, _)) in asn_map {
            let tags = AsnManager::extract_tags(&org);
            let category_str = ipverse_map.get(&asn).map(|s| s.as_str());
            let category = AsnManager::categorize_from_ipverse(&org, category_str);

            let tags_str = tags.join(",");
            let cat_str = match category {
                AsnCategory::Hosting => "hosting",
                AsnCategory::Residential => "residential",
                AsnCategory::Excluded => "excluded",
                AsnCategory::Unknown => "unknown",
            }
            .to_string();

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
        for chunk in asn_models.chunks(1000) {
            asns::Entity::insert_many(chunk.to_vec())
                .on_conflict(
                    sea_orm::sea_query::OnConflict::column(asns::Column::Asn)
                        .update_columns([asns::Column::Org, asns::Column::Category])
                        .value(
                            asns::Column::Country,
                            sea_orm::sea_query::Expr::cust(
                                "COALESCE(excluded.country, asns.country)",
                            ),
                        )
                        .value(
                            asns::Column::Tags,
                            sea_orm::sea_query::Expr::cust("COALESCE(excluded.tags, asns.tags)"),
                        )
                        .value(
                            asns::Column::LastUpdated,
                            sea_orm::sea_query::Expr::cust("CURRENT_TIMESTAMP"),
                        )
                        .to_owned(),
                )
                .exec(&*self.db)
                .await
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
        for chunk in range_models.chunks(5000) {
            asn_ranges::Entity::insert_many(chunk.to_vec())
                .on_conflict(
                    sea_orm::sea_query::OnConflict::column(asn_ranges::Column::Cidr)
                        .update_column(asn_ranges::Column::Asn)
                        .to_owned(),
                )
                .exec(&*self.db)
                .await
                .map_err(|e| AsnError::MaxMindError(format!("Range batch error: {}", e)))?;
        }

        tracing::info!("Global ASN discovery sync complete.");

        // Reload in-memory manager from the freshly imported database
        self.load_from_database().await?;

        Ok(())
    }

    /// Run a targeted batch re-categorization of "Unknown" ASNs using the ipverse map.
    /// Only processes ASNs that actually exist in the ipverse map to avoid iterating
    /// millions of unknowns that will never be categorized.
    pub async fn recategorize_all_asns(
        &self,
        ipverse_map: &std::collections::HashMap<String, String>,
    ) -> Result<usize, AsnError> {
        tracing::info!(
            "Starting recategorization of Unknown ASNs with ipverse map ({} entries)...",
            ipverse_map.len()
        );

        // Build the list of ASNs we can actually categorize (unknown + in ipverse map).
        // We filter the ipverse map keys to only those starting with "AS" for valid ASN format.
        let target_asns: Vec<String> = ipverse_map
            .keys()
            .filter(|k| k.starts_with("AS"))
            .cloned()
            .collect();

        if target_asns.is_empty() {
            tracing::info!("No target ASNs in ipverse map, skipping recategorization.");
            return Ok(0);
        }

        // Only fetch unknown ASNs that are in the ipverse map via SQL.
        // This avoids loading millions of rows into memory.
        let batch_size = 5000;
        let mut total_updated = 0;
        let mut processed = 0;
        let mut hosting_promoted = 0;
        let mut residential_promoted = 0;
        let mut excluded_promoted = 0;

        for chunk in target_asns.chunks(batch_size) {
            let batch_asns = asns::Entity::find()
                .filter(asns::Column::Category.eq("unknown"))
                .filter(asns::Column::Asn.is_in(chunk.iter().cloned()))
                .all(&*self.db)
                .await
                .map_err(|e| AsnError::MaxMindError(format!("DB error: {}", e)))?;

            if batch_asns.is_empty() {
                continue;
            }

            // Wrap the entire batch in a transaction for efficiency
            use sea_orm::TransactionTrait;
            let tx = self
                .db
                .begin()
                .await
                .map_err(|e| AsnError::MaxMindError(format!("Tx error: {}", e)))?;

            for model in batch_asns {
                processed += 1;
                let tags = AsnManager::extract_tags(&model.org);
                let category_str = ipverse_map.get(&model.asn).map(|s| s.as_str());
                let category = AsnManager::categorize_from_ipverse(&model.org, category_str);

                let mut active: asns::ActiveModel = model.into();
                let mut changed = false;

                if category != AsnCategory::Unknown {
                    let cat_str = match category {
                        AsnCategory::Hosting => {
                            hosting_promoted += 1;
                            "hosting"
                        }
                        AsnCategory::Residential => {
                            residential_promoted += 1;
                            "residential"
                        }
                        AsnCategory::Excluded => {
                            excluded_promoted += 1;
                            "excluded"
                        }
                        _ => "unknown",
                    };
                    active.category = Set(cat_str.to_string());
                    active.tags = Set(Some(tags.join(",")));
                    changed = true;
                }

                active.last_updated = Set(Some(Utc::now().into()));
                active
                    .update(&tx)
                    .await
                    .map_err(|e| AsnError::MaxMindError(format!("Update error: {}", e)))?;

                if changed {
                    total_updated += 1;
                }
            }

            tx.commit()
                .await
                .map_err(|e| AsnError::MaxMindError(format!("Commit error: {}", e)))?;
        }

        if processed > 0 {
            tracing::info!(
                "Recategorization complete: processed {} ASNs, {} promoted from Unknown ({} hosting, {} residential, {} excluded).",
                processed,
                total_updated,
                hosting_promoted,
                residential_promoted,
                excluded_promoted
            );
            // Refresh in-memory manager
            let _ = self.load_from_database().await;
        } else {
            tracing::info!("No unknown ASNs found in ipverse map to recategorize.");
        }

        Ok(total_updated)
    }

    /// Fetch ASN and Country data for a single IP using local MaxMind databases.
    ///
    /// This is a READ-ONLY lookup — it does NOT write to the database.
    /// All ASN/range data should come from the initial import (iptoasn + ipverse).
    /// MaxMind is used as a fallback for IPs not covered by imported ranges.
    pub async fn fetch_asn_for_ip(&self, ip: &str) -> Result<AsnRecord, AsnError> {
        let ip_addr: std::net::IpAddr = ip.parse().map_err(|_| AsnError::AsnNotFound)?;

        // 1. Try in-memory manager first (from initial import) — only for IPv4
        if let std::net::IpAddr::V4(v4) = ip_addr {
            let manager = self.asn_manager.read().await;
            if let Some(record) = manager.get_asn_for_ip(v4) {
                return Ok(record.clone());
            }
        }

        // 2. Fall back to MaxMind lookup (read-only, no DB writes)
        let asn_reader_lock = self.maxmind_asn.read().await;
        let mut asn_val = None;
        let mut org_val = None;

        if let Some(reader) = &*asn_reader_lock {
            if let Ok(result) = reader.lookup(ip_addr) {
                if let Ok(Some(asn_db)) = result.decode::<maxminddb::geoip2::Asn>() {
                    asn_val = asn_db.autonomous_system_number.map(|n| format!("AS{}", n));
                    org_val = asn_db
                        .autonomous_system_organization
                        .map(|s: &str| s.to_string());
                }
            }
        }
        drop(asn_reader_lock);

        // 3. Try local Country lookup
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

        // Try to get category from in-memory manager if we already know it
        let category = {
            let manager = self.asn_manager.read().await;
            manager.get_category(&asn)
        };
        let tags = AsnManager::extract_tags(&org);

        let record = AsnRecord {
            asn,
            org,
            category,
            country: country_code,
            last_updated: Some(Utc::now()),
            server_count: 0,
            tags,
        };

        // Cache in-memory only — do NOT pollute the database with scanner-discovered ASNs.
        // Only the initial import should create DB entries.
        {
            let mut manager = self.asn_manager.write().await;
            manager.add_asn(record.clone());

            // Cache /24 range in memory for future lookups
            if let std::net::IpAddr::V4(v4) = ip_addr {
                let octets = v4.octets();
                let cidr = format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]);
                manager.add_range(cidr, record.asn.clone());
            }
        }

        Ok(record)
    }

    /// Background task to periodically update databases.
    pub async fn run_background_refresh(self: Arc<Self>) {
        tracing::info!("Starting ASN intelligence background update task");

        // Initial delay to let startup settle
        time::sleep(time::Duration::from_secs(10)).await;

        let mut interval = time::interval(time::Duration::from_secs(86400 * 7)); // Weekly

        // Consume the first tick which happens immediately
        interval.tick().await;

        loop {
            interval.tick().await;

            // Check if we already have fresh ASN data (updated within last 3 days)
            // This is more reliable across restarts than just checking a flag.
            let fresh_asns = asns::Entity::find()
                .filter(asns::Column::LastUpdated.gt(Utc::now() - chrono::Duration::days(3)))
                .limit(1)
                .one(&*self.db)
                .await
                .unwrap_or_default();

            if fresh_asns.is_some() {
                tracing::info!(
                    "ASN data is fresh (updated within last 3 days), skipping background sync."
                );
                continue;
            }

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

            // 2. Refresh Full ASN Discovery Database (upsert only, not clean slate)
            let _ = self.import_full_database(false).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_range_to_cidrs_single_ip() {
        // Single IP: 10.0.0.1
        let cidrs = ip_range_to_cidrs(
            u32::from(Ipv4Addr::new(10, 0, 0, 1)),
            u32::from(Ipv4Addr::new(10, 0, 0, 1)),
        );
        assert_eq!(cidrs, vec!["10.0.0.1/32"]);
    }

    #[test]
    fn test_ip_range_to_cidrs_full_slash24() {
        // Full /24: 10.0.0.0 - 10.0.0.255
        let cidrs = ip_range_to_cidrs(
            u32::from(Ipv4Addr::new(10, 0, 0, 0)),
            u32::from(Ipv4Addr::new(10, 0, 0, 255)),
        );
        assert_eq!(cidrs, vec!["10.0.0.0/24"]);
    }

    #[test]
    fn test_ip_range_to_cidrs_full_slash16() {
        // Full /16: 10.0.0.0 - 10.0.255.255
        let cidrs = ip_range_to_cidrs(
            u32::from(Ipv4Addr::new(10, 0, 0, 0)),
            u32::from(Ipv4Addr::new(10, 0, 255, 255)),
        );
        assert_eq!(cidrs, vec!["10.0.0.0/16"]);
    }

    #[test]
    fn test_ip_range_to_cidrs_unaligned() {
        // 10.0.0.100 - 10.0.0.200 (non-aligned, should produce multiple CIDRs)
        let cidrs = ip_range_to_cidrs(
            u32::from(Ipv4Addr::new(10, 0, 0, 100)),
            u32::from(Ipv4Addr::new(10, 0, 0, 200)),
        );
        // Should cover the range exactly with multiple CIDRs
        let total_ips: u32 = cidrs
            .iter()
            .map(|c| {
                let prefix: u8 = c.split('/').nth(1).unwrap().parse().unwrap();
                1u32 << (32 - prefix)
            })
            .sum();
        assert_eq!(total_ips, 101); // 200 - 100 + 1 = 101 IPs
    }

    #[test]
    fn test_ip_range_to_cidrs_cross_octet() {
        // 10.0.0.200 - 10.0.1.50 (crosses /24 boundary)
        let cidrs = ip_range_to_cidrs(
            u32::from(Ipv4Addr::new(10, 0, 0, 200)),
            u32::from(Ipv4Addr::new(10, 0, 1, 50)),
        );
        let total_ips: u32 = cidrs
            .iter()
            .map(|c| {
                let prefix: u8 = c.split('/').nth(1).unwrap().parse().unwrap();
                1u32 << (32 - prefix)
            })
            .sum();
        // 56 (200-255) + 51 (0-50) = 107 IPs... wait: 256-200=56, 50-0+1=51 = 107
        // Actually: 10.0.0.200 to 10.0.0.255 = 56 IPs, 10.0.1.0 to 10.0.1.50 = 51 IPs
        // Total = 107
        assert_eq!(total_ips, 107);
    }

    #[test]
    fn test_ip_range_to_cidrs_large_range() {
        // 10.0.0.0 - 10.0.7.255 = /21 (2048 IPs)
        let cidrs = ip_range_to_cidrs(
            u32::from(Ipv4Addr::new(10, 0, 0, 0)),
            u32::from(Ipv4Addr::new(10, 0, 7, 255)),
        );
        assert_eq!(cidrs, vec!["10.0.0.0/21"]);
    }
}
