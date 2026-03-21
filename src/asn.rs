//! ASN (Autonomous System Number) module for categorizing IP ranges.
//!
//! Provides ASN lookup, categorization (Hosting/Residential/Unknown),
//! and efficient IP-to-ASN mapping.

use chrono::{DateTime, Utc};
use ipnetwork::Ipv4Network;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use thiserror::Error;

/// ASN category for scan prioritization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "lowercase")]
pub enum AsnCategory {
    /// VPS/Cloud providers - scanned frequently (2-4 times/day)
    Hosting,
    /// Residential ISPs - scanned rarely (1-2 times/month)
    Residential,
    /// Unknown or unclassified
    Unknown,
}

impl AsnCategory {
    /// Get category priority (lower = higher priority).
    pub fn priority(&self) -> i32 {
        match self {
            AsnCategory::Hosting => 1,
            AsnCategory::Residential => 3,
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

/// Known hosting providers for categorization.
/// These ASNs are prioritized for Warm scans.
const HOSTING_ASN_PREFIXES: &[&str] = &[
    // AWS
    "AS16509", // AMAZON-02
    "AS14618", // AMAZON-AES
    "AS8987",  // AMAZON-AES-EU
    // Google Cloud
    "AS15169", // GOOGLE
    "AS36040", // GOOGLE-CLOUD
    "AS36384", // GOOGLE-CLOUD-2
    // Microsoft Azure
    "AS8075",  // MICROSOFT-CORP
    "AS3598",  // MICROSOFT-CORP-2
    // Hetzner
    "AS24940", // HETZNER-ONLINE
    // OVH
    "AS16276", // OVH
    // DigitalOcean
    "AS14061", // DIGITALOCEAN-ASN
    // Linode
    "AS63949", // LINODE-AP
    "AS6939",  // HURRICANE
    // Vultr
    "AS20473", // AS-CHOOPA
    // Cloudflare
    "AS13335", // CLOUDFLARENET
    // Scaleway
    "AS12876", // SCALEWAY
    // Online.net
    "AS12322", // FREE
    // Leaseweb
    "AS60781", // LEASEWEB-NL
    "AS35280", // F5
    // Contabo
    "AS51167", // CONTABO
    // Ionos
    "AS8560",  // IONOS
    // Rackspace
    "AS27357", // RACKSPACE
    // Oracle Cloud
    "AS31898", // ORACLE-BMC
    "AS13220", // TUCOWS
];

/// Known residential ISPs.
const RESIDENTIAL_ASN_PREFIXES: &[&str] = &[
    // Comcast
    "AS7922",
    // Verizon
    "AS701",
    "AS702",
    "AS703",
    // AT&T
    "AS20115",
    "AS26809",
    // Spectrum
    "AS11351",
    "AS12271",
    // Cox
    "AS22773",
    // Deutsche Telekom
    "AS3320",
    // BT
    "AS2856",
    // Orange
    "AS3215",
    // KPN
    "AS1136",
    // Vodafone
    "AS6830",
    // Sky
    "AS5607",
    // T-Mobile
    "AS21928",
    // JCOM (Japan)
    "AS17676",
    // NTT
    "AS2914",
    // Telstra
    "AS1221",
];

/// ASN manager for looking up and categorizing ASNs.
pub struct AsnManager {
    /// ASN records by ASN number.
    asns: HashMap<String, AsnRecord>,
    /// IP ranges indexed by ASN.
    ranges: Vec<AsnRange>,
    /// Category lookup by ASN prefix.
    hosting_prefixes: Vec<String>,
    residential_prefixes: Vec<String>,
}

impl AsnManager {
    pub fn new() -> Self {
        Self {
            asns: HashMap::new(),
            ranges: Vec::new(),
            hosting_prefixes: HOSTING_ASN_PREFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            residential_prefixes: RESIDENTIAL_ASN_PREFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    /// Add an ASN record.
    pub fn add_asn(&mut self, record: AsnRecord) {
        self.asns.insert(record.asn.clone(), record);
    }

    /// Add an IP range for an ASN.
    pub fn add_range(&mut self, cidr: String, asn: String) {
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

        // Fallback: check prefixes
        if self
            .hosting_prefixes
            .iter()
            .any(|prefix| asn.starts_with(prefix))
        {
            return AsnCategory::Hosting;
        }

        if self
            .residential_prefixes
            .iter()
            .any(|prefix| asn.starts_with(prefix))
        {
            return AsnCategory::Residential;
        }

        AsnCategory::Unknown
    }

    /// Get all hosting ASNs.
    pub fn get_hosting_asns(&self) -> Vec<&AsnRecord> {
        self.asns
            .values()
            .filter(|r| r.category == AsnCategory::Hosting)
            .collect()
    }

    /// Get all residential ASNs.
    pub fn get_residential_asns(&self) -> Vec<&AsnRecord> {
        self.asns
            .values()
            .filter(|r| r.category == AsnCategory::Residential)
            .collect()
    }

    /// Get ranges for a specific category.
    pub fn get_ranges_by_category(&self, category: AsnCategory) -> Vec<&AsnRange> {
        self.ranges
            .iter()
            .filter(|r| {
                self.asns
                    .get(&r.asn)
                    .map(|rec| rec.category == category)
                    .unwrap_or(false)
            })
            .collect()
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
    pub fn categorize_by_org(org: &str) -> AsnCategory {
        let org_lower = org.to_lowercase();

        // Hosting providers
        let hosting_keywords = [
            "amazon", "aws", "google", "microsoft", "azure", "hetzner", "ovh",
            "digitalocean", "linode", "vultr", "cloudflare", "scaleway", "online.net",
            "leaseweb", "contabo", "ionos", "rackspace", "oracle", "cloud", "hosting",
            "datacenter", "server", "vps", "dedicated", "colo",
        ];

        // Residential providers
        let residential_keywords = [
            "comcast", "verizon", "at&t", "spectrum", "cox", "telekom", "bt ", "orange",
            "kpn", "vodafone", "sky ", "t-mobile", "jcom", "ntt ", "telstra",
            "broadband", "cable", "fiber", "isp", "residential",
        ];

        for keyword in &hosting_keywords {
            if org_lower.contains(keyword) {
                return AsnCategory::Hosting;
            }
        }

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

/// Response from ipapi.co API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpApiResponse {
    pub ip: String,
    pub asn: Option<String>,
    pub org: Option<String>,
    pub network: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Error, Debug)]
pub enum AsnError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("IP network error: {0}")]
    IpNetworkError(#[from] ipnetwork::IpNetworkError),
    #[error("ASN not found")]
    AsnNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_hosting() {
        assert_eq!(
            AsnManager::categorize_by_org("AMAZON-02"),
            AsnCategory::Hosting
        );
        assert_eq!(
            AsnManager::categorize_by_org("Hetzner Online GmbH"),
            AsnCategory::Hosting
        );
        assert_eq!(
            AsnManager::categorize_by_org("Google Cloud"),
            AsnCategory::Hosting
        );
    }

    #[test]
    fn test_categorize_residential() {
        assert_eq!(
            AsnManager::categorize_by_org("Comcast Cable"),
            AsnCategory::Residential
        );
        assert_eq!(
            AsnManager::categorize_by_org("Verizon Business"),
            AsnCategory::Residential
        );
    }

    #[test]
    fn test_categorize_unknown() {
        assert_eq!(
            AsnManager::categorize_by_org("Some Random Company"),
            AsnCategory::Unknown
        );
    }

    #[test]
    fn test_asn_range_contains() {
        let range = AsnRange::new("192.168.0.0/24".to_string(), "AS123".to_string()).unwrap();
        assert!(range.contains(Ipv4Addr::new(192, 168, 0, 1)));
        assert!(range.contains(Ipv4Addr::new(192, 168, 0, 254)));
        assert!(!range.contains(Ipv4Addr::new(192, 168, 1, 1)));
    }
}
