//! ASN (Autonomous System Number) module for categorizing IP ranges.
//!
//! Provides ASN lookup, categorization (Hosting/Residential/Unknown),
//! and efficient IP-to-ASN mapping.

use chrono::{DateTime, Utc};
use ipnetwork::Ipv4Network;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;

/// ASN category for scan prioritization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[allow(dead_code)]
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
    #[serde(default)]
    pub tags: Vec<String>,
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
        Ok(Self { cidr, asn, network })
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
    /// Set of CIDRs already in ranges (for O(1) dedup).
    range_set: HashSet<String>,
}

impl AsnManager {
    pub fn new() -> Self {
        Self {
            asns: HashMap::new(),
            ranges: Vec::new(),
            range_set: HashSet::new(),
        }
    }

    /// Add an ASN record.
    pub fn add_asn(&mut self, record: AsnRecord) {
        self.asns.insert(record.asn.clone(), record);
    }

    /// Add an IP range for an ASN.
    pub fn add_range(&mut self, cidr: String, asn: String) {
        if cidr.contains(':') {
            return;
        } // Skip IPv6
        if self.range_set.contains(&cidr) {
            return;
        } // Skip duplicates
        if let Ok(range) = AsnRange::new(cidr.clone(), asn) {
            self.range_set.insert(cidr);
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
    #[allow(dead_code)]
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

    /// Get ASN record by ASN number (O(1)).
    #[allow(dead_code)]
    pub fn get_asn(&self, asn: &str) -> Option<&AsnRecord> {
        self.asns.get(asn)
    }

    /// Map an ipverse category string to an AsnCategory with a safety keyword override.
    pub fn categorize_from_ipverse(org: &str, category: Option<&str>) -> AsnCategory {
        let org_lower = org.to_lowercase();

        // 1. CRITICAL SAFETY BLOCKLIST (Military, Gov, Edu, Infrastructure, Honeypots, Scanners)
        // These keywords override any external categorization for safety.
        let safety_keywords = [
            // Military, Government, Defense
            "military",
            "defense",
            "dod",
            "pentagon",
            "army",
            "navy",
            "air force",
            "marines",
            "national security",
            "intelligence",
            "government",
            "gov.",
            "ministry",
            "federal",
            "police",
            "fbi",
            "cia",
            "nsa",
            // Education & Healthcare
            "university",
            "college",
            "school",
            "academy",
            "hospital",
            "medical",
            "clinic",
            // Critical Infrastructure
            "nuclear",
            "atomic",
            "power plant",
            "bank",
            "financial",
            "securities",
            "reserve",
            // HONEYPOTS & SECURITY RESEARCH (CRITICAL - Must Never Scan)
            "honeypot",
            "honey pot",
            "honey-net",
            "honeynet",
            "research network",
            "background noise",
            "security research",
            "threat intel",
            "threat intelligence",
            "grey noise",
            "gray noise",
            "project honey",
            "censys",
            "shodan",
            "zmap",
            "rapid7",
            "fofa",
            "quake",
            "360 netlab",
            "netlab360",
            "antiscan",
            "internet census",
            "sensor network",
            "monitoring service",
            "abuse monitoring",
            "skhron",
            "datalix",
            "cloudflare radar",
            "bgpstream",
            "shadowserver",
            "spamhaus",
            "abuseipdb",
            "team cymru",
            "binaryedge",
            "fullhunt",
            "zoomeye",
            "leakix",
            "onyphe",
            "pulsedive",
            "virustotal",
            "urlscan",
            "internet scanning",
            "mass scanner",
            "port scanner",
            "vulnerability scanner",
            "attack surface",
            "scanning service",
            "internet scanner",
            "research project",
            "security scanner",
            "network monitor",
            // Additional Scanner Organizations (specific names, NOT generic CDN/hosting)
            "sonar research",
            "opendns",
            "umbrella security",
            // Research & Census Projects
            "isc",
            "internet systems consortium",
            "caida",
            "routeviews",
            "ripe ncc",
            // IP Geolocation/Intelligence Services (often used for scanning)
            "maxmind",
            "ip2location",
            "db-ip",
        ];

        for keyword in &safety_keywords {
            if org_lower.contains(keyword) {
                return AsnCategory::Excluded;
            }
        }

        // 2. Map ipverse category
        match category {
            Some("hosting") => AsnCategory::Hosting,
            Some("isp") | Some("business") => AsnCategory::Residential,
            Some("education_research") | Some("government_admin") => AsnCategory::Excluded,
            _ => AsnCategory::Unknown,
        }
    }

    /// Extract descriptive tags based on organization name.
    pub fn extract_tags(org: &str) -> Vec<String> {
        let org_lower = org.to_lowercase();
        let mut tags = Vec::new();

        let ddos_keywords = [
            "ddos",
            "shield",
            "protect",
            "scrub",
            "mitigation",
            "voxility",
            "path.net",
            "stormwall",
            "cloudefense",
        ];
        for k in &ddos_keywords {
            if org_lower.contains(k) {
                tags.push("DDoS-Protected".to_string());
                break;
            }
        }

        let cloud_keywords = [
            "amazon",
            "aws",
            "google",
            "microsoft",
            "azure",
            "cloud",
            "compute",
            "instance",
            "stack",
            "lambda",
        ];
        for k in &cloud_keywords {
            if org_lower.contains(k) {
                tags.push("Cloud".to_string());
                break;
            }
        }

        let cdn_keywords = [
            "cloudflare",
            "akamai",
            "fastly",
            "cdn",
            "edgecast",
            "limelight",
            "bunny",
        ];
        for k in &cdn_keywords {
            if org_lower.contains(k) {
                tags.push("CDN".to_string());
                break;
            }
        }

        tags.dedup();
        tags
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_from_ipverse() {
        assert_eq!(
            AsnManager::categorize_from_ipverse("Digital Ocean", Some("hosting")),
            AsnCategory::Hosting
        );
        assert_eq!(
            AsnManager::categorize_from_ipverse("Comcast", Some("isp")),
            AsnCategory::Residential
        );

        // Safety keyword override tests
        assert_eq!(
            AsnManager::categorize_from_ipverse("Department of Defense", Some("business")),
            AsnCategory::Excluded
        );
        assert_eq!(
            AsnManager::categorize_from_ipverse("Harvard University", Some("education_research")),
            AsnCategory::Excluded
        );
        assert_eq!(
            AsnManager::categorize_from_ipverse("US Air Force", None),
            AsnCategory::Excluded
        );

        // Regular categories
        assert_eq!(
            AsnManager::categorize_from_ipverse("Normal Biz", Some("business")),
            AsnCategory::Residential
        );
        assert_eq!(
            AsnManager::categorize_from_ipverse("Unknown Org", Some("unknown_cat")),
            AsnCategory::Unknown
        );
        assert_eq!(
            AsnManager::categorize_from_ipverse("Nothing", None),
            AsnCategory::Unknown
        );
    }

    #[test]
    fn test_extract_tags() {
        let tags = AsnManager::extract_tags("Amazon Cloud Services");
        assert_eq!(tags, vec!["Cloud"]);

        let tags2 = AsnManager::extract_tags("Cloudflare CDN");
        assert_eq!(tags2, vec!["Cloud", "CDN"]); // "Cloud" matches "cloud", "CDN" matches "cloudflare" and "cdn" (deduped)

        let tags3 = AsnManager::extract_tags("Comcast");
        assert!(tags3.is_empty());
    }
}
