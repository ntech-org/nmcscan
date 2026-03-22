//! ASN (Autonomous System Number) module for categorizing IP ranges.
//!
//! Provides ASN lookup, categorization (Hosting/Residential/Unknown),
//! and efficient IP-to-ASN mapping.

use chrono::{DateTime, Utc};
use ipnetwork::Ipv4Network;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::Ipv4Addr;

/// ASN category for scan prioritization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "lowercase")]
pub enum AsnCategory {
    /// VPS/Cloud providers - scanned frequently (2-4 times/day)
    Hosting,
    /// Residential ISPs - scanned rarely (1-2 times/month)
    Residential,
    /// Sensitive/Restricted networks (Military, Gov, Edu) - NEVER scanned
    Excluded,
    /// Unknown or unclassified
    Unknown,
}

impl AsnCategory {
    /// Get category priority (lower = higher priority).
    pub fn priority(&self) -> i32 {
        match self {
            AsnCategory::Hosting => 1,
            AsnCategory::Residential => 3,
            AsnCategory::Excluded => 99, // Should not be scanned
            AsnCategory::Unknown => 2,
        }
    }
}

/// ASN record from database or API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsnRecord {
    pub asn: String,
    pub org: String,
    pub category: AsnCategory,
    pub country: Option<String>,
    pub last_updated: Option<DateTime<Utc>>,
    pub server_count: i64,
}

/// IP range with associated ASN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsnRange {
    pub cidr: String,
    pub asn: String,
    pub network: Ipv4Network,
}

impl AsnRange {
    pub fn new(cidr: String, asn: String) -> Result<Self, ipnetwork::IpNetworkError> {
        let network = cidr.parse::<Ipv4Network>()?;
        Ok(Self {
            cidr,
            asn,
            network,
        })
    }

    /// Check if an IP is in this range.
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        self.network.contains(ip)
    }
}

/// ASN manager for looking up and categorizing ASNs.
pub struct AsnManager {
    /// ASN records by ASN number.
    asns: HashMap<String, AsnRecord>,
    /// IP ranges indexed by ASN.
    ranges: Vec<AsnRange>,
}

impl AsnManager {
    pub fn new() -> Self {
        Self {
            asns: HashMap::new(),
            ranges: Vec::new(),
        }
    }

    /// Add an ASN record.
    pub fn add_asn(&mut self, record: AsnRecord) {
        self.asns.insert(record.asn.clone(), record);
    }

    /// Add an IP range for an ASN.
    pub fn add_range(&mut self, cidr: String, asn: String) {
        if cidr.contains(':') { return; } // Skip IPv6
        if let Ok(range) = AsnRange::new(cidr, asn) {
            self.ranges.push(range);
        }
    }

    /// Get ASN for an IP address by checking ranges.
    pub fn get_asn_for_ip(&self, ip: Ipv4Addr) -> Option<&AsnRecord> {
        for range in &self.ranges {
            if range.contains(ip) {
                return self.asns.get(&range.asn);
            }
        }
        None
    }

    /// Get category for an ASN.
    pub fn get_category(&self, asn: &str) -> AsnCategory {
        if let Some(record) = self.asns.get(asn) {
            return record.category.clone();
        }
        AsnCategory::Unknown
    }

    /// Get the number of ASNs loaded.
    pub fn asn_count(&self) -> usize {
        self.asns.len()
    }

    /// Get the number of ranges loaded.
    pub fn range_count(&self) -> usize {
        self.ranges.len()
    }

    /// Categorize an ASN based on organization name.
    /// SAFETY: This is the primary gatekeeper for the scanner.
    pub fn categorize_by_org(org: &str) -> AsnCategory {
        let org_lower = org.to_lowercase();

        // 1. CRITICAL SAFETY BLOCKLIST (Military, Gov, Edu, Infrastructure)
        let blocked_keywords = [
            // Military & Defense
            "military", "defense", "dod", "pentagon", "army", "navy", "air force", "marines",
            "department of defense", "national security", "intelligence", "signal corps",
            // Government
            "government", "gov.", "ministry", "federal", "state of", "city of", "municipality",
            "parliament", "congress", "senate", "white house", "official",
            // Law Enforcement
            "police", "law enforcement", "fbi", "cia", "nsa", "justice", "sheriff", "interpol",
            // Education (Universities/Schools)
            "university", "college", "school", "academy", "institute of technology", ".edu", 
            "higher education", "campus", "district",
            // Critical Infrastructure
            "hospital", "medical center", "healthcare", "clinic", "emergency", "911", "ambulance",
            "nuclear", "atomic", "energy department", "power plant", "water works",
            "bank", "financial", "securities", "reserve", "exchange", "investment",
        ];

        for keyword in &blocked_keywords {
            if org_lower.contains(keyword) {
                return AsnCategory::Excluded;
            }
        }

        // 2. Hosting providers
        let hosting_keywords = [
            "amazon", "aws", "google", "microsoft", "azure", "hetzner", "ovh",
            "digitalocean", "linode", "vultr", "cloudflare", "scaleway", "online.net",
            "leaseweb", "contabo", "ionos", "rackspace", "oracle", "cloud", "hosting",
            "datacenter", "server", "vps", "dedicated", "colo", "compute", "instance",
            "stack", "packet", "infrastructure", "liquid web", "choopa", "akamai",
        ];

        for keyword in &hosting_keywords {
            if org_lower.contains(keyword) {
                return AsnCategory::Hosting;
            }
        }

        //  residential providers
        let residential_keywords = [
            "comcast", "verizon", "at&t", "spectrum", "cox", "telekom", "bt ", "orange",
            "kpn", "vodafone", "sky ", "t-mobile", "jcom", "ntt ", "telstra",
            "broadband", "cable", "fiber", "isp", "residential", "consumer",
            "home", "dsl", "wireless", "mobile", "lte", "5g", "customer",
        ];

        for keyword in &residential_keywords {
            if org_lower.contains(keyword) {
                return AsnCategory::Residential;
            }
        }

        AsnCategory::Unknown
    }
}

impl Default for AsnManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AsnError {
    #[error("Database error: {0}")]
    MaxMindError(String),
    #[error("IP network error: {0}")]
    IpNetworkError(#[from] ipnetwork::IpNetworkError),
    #[error("ASN not found")]
    AsnNotFound,
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
}
