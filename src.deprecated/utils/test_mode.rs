//! Test mode configuration for safe home testing.
//!
//! Provides a list of known public Minecraft servers for testing
//! the scanner without scanning random IP ranges.

use serde::{Deserialize, Serialize};

/// Known public Minecraft servers for testing.
/// These are well-known servers that explicitly allow connections.
pub const KNOWN_MINECRAFT_SERVERS: &[(&str, u16, &str, &str)] = &[
    // Hypixel
    ("172.65.197.160", 25565, "Hypixel", "mc.hypixel.net"),
    // Wynncraft
    ("172.65.217.91", 25565, "Wynncraft", "play.wynncraft.com"),
    // GommeHD
    ("141.95.62.90", 25565, "Gommehd", "mc.gommehd.net"),
    // ManaCube
    ("51.79.44.42", 25565, "ManaCube", "play.manacube.com"),
    // CubeCraft
    ("193.105.184.227", 25565, "CubeCraft", "play.cubecraft.net"),
    // Herobrine.org
    ("51.89.162.115", 25565, "Herobrine", "herobrine.org"),
    // PikaNetwork
    (
        "135.125.131.133",
        25565,
        "PikaNetwork",
        "play.pika-network.net",
    ),
];

/// Test configuration options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Enable test mode (only scan known servers)
    pub enabled: bool,
    /// Only scan servers from specific regions
    pub regions: Vec<String>,
    /// Maximum number of servers to scan in test mode
    pub max_servers: usize,
    /// Scan interval in seconds for test mode
    pub scan_interval: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            regions: vec![],
            max_servers: 50,
            scan_interval: 60, // 1 minute between full test scans
        }
    }
}

impl TestConfig {
    /// Create a new test config from environment variables.
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("TEST_MODE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            max_servers: std::env::var("TEST_MAX_SERVERS")
                .and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent))
                .unwrap_or(50),
            scan_interval: std::env::var("TEST_SCAN_INTERVAL")
                .and_then(|v| v.parse().map_err(|_| std::env::VarError::NotPresent))
                .unwrap_or(60),
            regions: std::env::var("TEST_REGIONS")
                .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
        }
    }

    /// Get test servers based on configuration.
    pub fn get_test_servers(&self) -> Vec<(String, u16, String, String)> {
        KNOWN_MINECRAFT_SERVERS
            .iter()
            .take(self.max_servers)
            .map(|(ip, port, name, host)| {
                (ip.to_string(), *port, name.to_string(), host.to_string())
            })
            .collect()
    }
}

/// Get a small subset of servers for quick testing.
pub fn get_quick_test_servers() -> Vec<(String, u16, String, String)> {
    // Return most reliable servers for quick testing
    vec![
        ("172.65.197.160", 25565, "Hypixel", "mc.hypixel.net"),
        ("172.65.217.91", 25565, "Wynncraft", "play.wynncraft.com"),
        ("141.95.62.90", 25565, "Gommehd", "mc.gommehd.net"),
        ("51.79.44.42", 25565, "ManaCube", "play.manacube.com"),
        ("193.105.184.227", 25565, "CubeCraft", "play.cubecraft.net"),
    ]
    .into_iter()
    .map(|(ip, port, name, host)| (ip.to_string(), port, name.to_string(), host.to_string()))
    .collect()
}

/// Get servers by region/category.
pub fn get_servers_by_region(region: &str) -> Vec<(String, u16, String, String)> {
    match region.to_lowercase().as_str() {
        "us" | "usa" | "america" => KNOWN_MINECRAFT_SERVERS
            .iter()
            .filter(|(_, _, name, _)| {
                name.contains("Hypixel") || name.contains("Mineplex") || name.contains("Test")
            })
            .map(|(ip, port, name, host)| {
                (ip.to_string(), *port, name.to_string(), host.to_string())
            })
            .collect(),
        "eu" | "europe" => KNOWN_MINECRAFT_SERVERS
            .iter()
            .filter(|(_, _, name, _)| {
                name.contains("Gommehd")
                    || name.contains("Redstone")
                    || name.contains("Hermitcraft")
                    || name.contains("2b2t")
            })
            .map(|(ip, port, name, host)| {
                (ip.to_string(), *port, name.to_string(), host.to_string())
            })
            .collect(),
        _ => KNOWN_MINECRAFT_SERVERS
            .iter()
            .map(|(ip, port, name, host)| {
                (ip.to_string(), *port, name.to_string(), host.to_string())
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_servers_not_empty() {
        assert!(!KNOWN_MINECRAFT_SERVERS.is_empty());
        assert!(KNOWN_MINECRAFT_SERVERS.len() >= 5);
    }

    #[test]
    fn test_quick_test_servers() {
        let servers = get_quick_test_servers();
        assert_eq!(servers.len(), 5);
        // First server should be Hypixel
        assert!(servers[0].2.contains("Hypixel"));
    }

    #[test]
    fn test_get_servers_by_region_us() {
        let servers = get_servers_by_region("us");
        assert!(!servers.is_empty());
        assert!(servers
            .iter()
            .any(|(_, _, name, _)| name.contains("Hypixel")));
    }

    #[test]
    fn test_get_servers_by_region_eu() {
        let servers = get_servers_by_region("eu");
        assert!(!servers.is_empty());
        assert!(servers
            .iter()
            .any(|(_, _, name, _)| name.contains("Gommehd")));
    }

    #[test]
    fn test_test_config_default() {
        let config = TestConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_servers, 50);
        assert_eq!(config.scan_interval, 60);
    }

    #[test]
    fn test_test_config_from_env() {
        // This tests that the config can be created from env
        // (even if env vars aren't set, it should use defaults)
        let config = TestConfig::from_env();
        assert!(config.max_servers > 0);
        assert!(config.scan_interval > 0);
    }
}
