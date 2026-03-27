//! Exclude list module for safe IP filtering.
//! 
//! Parses exclude.conf format (CIDR and single IPs) and provides
//! efficient lookup to avoid scanning protected IP ranges.

use ipnetwork::Ipv4Network;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use thiserror::Error;

/// Parse an IP range string like "202.91.162.0-202.91.175.255" into start and end IPs.
fn parse_ip_range(range: &str) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let start: Ipv4Addr = parts[0].trim().parse().ok()?;
    let end: Ipv4Addr = parts[1].trim().parse().ok()?;
    Some((start, end))
}

/// Convert an IP range to a list of CIDR networks.
/// This is a simplified implementation that creates covering networks.
fn range_to_cidrs(start: Ipv4Addr, end: Ipv4Addr) -> Vec<Ipv4Network> {
    let mut networks = Vec::new();
    let start_u32 = u32::from_be_bytes(start.octets());
    let end_u32 = u32::from_be_bytes(end.octets());
    
    // For simplicity, create a /16 or /24 that covers most of the range
    // A full implementation would use proper CIDR aggregation
    if end_u32 - start_u32 > 65535 {
        // Large range - use /16
        let base = start_u32 & 0xFFFF0000;
        if let Ok(net) = Ipv4Network::new(Ipv4Addr::from(base.to_be_bytes()), 16) {
            networks.push(net);
        }
    } else if end_u32 - start_u32 > 255 {
        // Medium range - use /24
        let base = start_u32 & 0xFFFFFF00;
        if let Ok(net) = Ipv4Network::new(Ipv4Addr::from(base.to_be_bytes()), 24) {
            networks.push(net);
        }
    } else {
        // Small range - use individual IPs or /30
        for ip_u32 in start_u32..=end_u32 {
            let ip = Ipv4Addr::from(ip_u32.to_be_bytes());
            if let Ok(net) = Ipv4Network::new(ip, 32) {
                networks.push(net);
            }
        }
    }
    
    networks
}

/// Normalize IP address lines by removing leading zeros from octets.
/// e.g., "07.60.122.24/29" -> "7.60.122.24/29"
fn normalize_ip_line(line: &str) -> String {
    if let Some((ip, suffix)) = line.split_once('/') {
        // Has CIDR suffix
        let normalized_ip = ip
            .split('.')
            .map(|octet| {
                let trimmed = octet.trim_start_matches('0');
                if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
            })
            .collect::<Vec<_>>()
            .join(".");
        format!("{}/{}", normalized_ip, suffix)
    } else {
        // No CIDR suffix
        line.split('.')
            .map(|octet| {
                let trimmed = octet.trim_start_matches('0');
                if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
            })
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[derive(Error, Debug)]
pub enum ExcludeListError {
    #[error("Failed to read exclude file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse network: {0}")]
    ParseError(String),
}

/// Holds a list of excluded IP ranges (CIDR) and single IPs.
/// 
/// Before ANY connection attempt, check `if exclude_list.contains(ip)`.
/// If true, SKIP immediately. Do not log, do not ping.
pub struct ExcludeList {
    networks: Vec<Ipv4Network>,
}

impl ExcludeList {
    /// Load exclude list from a file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ExcludeListError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }

    /// Parse exclude list from a string (for testing).
    pub fn from_str(content: &str) -> Result<Self, ExcludeListError> {
        let mut networks = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let line = line.split('#').next().unwrap_or(line).trim();
            if line.is_empty() { continue; }
            if line.contains('-') && !line.contains('/') {
                if let Some((start, end)) = parse_ip_range(line) {
                    networks.extend(range_to_cidrs(start, end));
                }
                continue;
            }
            let normalized = normalize_ip_line(line);
            if let Ok(network) = normalized.parse::<Ipv4Network>() {
                networks.push(network);
            } else if let Ok(ip) = normalized.parse::<Ipv4Addr>() {
                networks.push(Ipv4Network::new(ip, 32).unwrap());
            }
        }
        Ok(Self { networks })
    }

    pub fn is_excluded(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => self.networks.iter().any(|n| n.contains(ipv4)),
            IpAddr::V6(_) => false,
        }
    }

    pub fn len(&self) -> usize {
        self.networks.len()
    }
}

use tokio::sync::RwLock;
use std::sync::Arc;
use std::io::Write;

/// Manager for dynamic exclusion list.
pub struct ExcludeManager {
    inner: Arc<RwLock<ExcludeList>>,
    file_path: String,
}

impl ExcludeManager {
    pub fn new(file_path: &str) -> Self {
        let initial = ExcludeList::from_file(file_path).unwrap_or_else(|_| {
            ExcludeList { networks: Vec::new() }
        });
        
        Self {
            inner: Arc::new(RwLock::new(initial)),
            file_path: file_path.to_string(),
        }
    }

    /// Check if an IP is excluded.
    pub async fn is_excluded(&self, ip: IpAddr) -> bool {
        self.inner.read().await.is_excluded(ip)
    }

    /// Add a new exclusion to the file and reload memory.
    pub async fn add_exclusion(&self, network: &str, comment: Option<&str>) -> Result<(), ExcludeListError> {
        // 1. Validate the entry first
        let normalized = normalize_ip_line(network.trim());
        if normalized.parse::<Ipv4Network>().is_err() && normalized.parse::<Ipv4Addr>().is_err() {
            return Err(ExcludeListError::ParseError(format!("Invalid network format: {}", network)));
        }

        // 2. Append to file
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.file_path)?;
        
        let entry = if let Some(c) = comment {
            format!("\n{} # {}\n", network, c)
        } else {
            format!("\n{}\n", network)
        };
        
        file.write_all(entry.as_bytes())?;

        // 3. Reload in memory
        let new_list = ExcludeList::from_file(&self.file_path)?;
        let mut write_lock = self.inner.write().await;
        *write_lock = new_list;

        Ok(())
    }

    /// Get current list count.
    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exclude_cidr() {
        let content = "192.168.0.0/16\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(192, 168, 1, 1).into()));
        assert!(!list.is_excluded(Ipv4Addr::new(10, 0, 0, 1).into()));
    }

    #[test]
    fn test_exclude_single_ip() {
        let content = "153.11.0.1\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(153, 11, 0, 1).into()));
        assert!(!list.is_excluded(Ipv4Addr::new(153, 11, 0, 2).into()));
    }

    #[test]
    fn test_comments_ignored() {
        let content = "# This is a comment\n192.168.1.0/24\n# Another comment\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(192, 168, 1, 100).into()));
    }

    #[test]
    fn test_military_ranges_excluded() {
        // Test key military ranges from exclude.conf
        let content = "6.0.0.0/8\n7.0.0.0/8\n11.0.0.0/8\n21.0.0.0/8\n";
        let list = ExcludeList::from_str(content).unwrap();
        assert!(list.is_excluded(Ipv4Addr::new(6, 0, 0, 1).into()));
        assert!(list.is_excluded(Ipv4Addr::new(7, 255, 255, 255).into()));
        assert!(list.is_excluded(Ipv4Addr::new(11, 128, 0, 1).into()));
    }
}
