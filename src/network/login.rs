//! Minecraft Login protocol implementation for offline-mode login testing.
//!
//! Attempts an offline login (username "NMCScan") to determine if a server
//! accepts cracked/offline accounts. Supports Minecraft 1.7 (protocol 5)
//! through 26.1 (protocol 775).
//!
//! Protocol flow:
//! 1. Handshake with next_state = 2 (Login)
//! 2. Login Start with username "NMCScan"
//! 3. Read server response:
//!    - 0x00 Disconnect → classify reason (whitelist/banned/rejected/version mismatch)
//!    - 0x01 Encryption Request → online-mode only (premium)
//!    - 0x02 Login Success → offline mode enabled (cracked)
//!    - 0x03 Set Compression → read threshold, then expect Login Success

use once_cell::sync::Lazy;
use regex::Regex;
use std::io;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

/// The username used for offline login attempts.
const OFFLINE_USERNAME: &str = "NMCScan";

/// Maximum packet size for safety (256KB).
const MAX_PACKET_SIZE: usize = 256 * 1024;

/// Latest supported protocol version.
pub const LATEST_PROTOCOL: i32 = 775;

/// Mapping of version name patterns to protocol versions.
/// Source: https://minecraft.wiki/w/Protocol_version and https://wiki.vg/Protocol_version_numbers
/// Ordered from newest to oldest for efficient lookup.
const VERSION_PROTOCOL_MAP: &[(i32, &[&str])] = &[
    // 2026 - Year-based versioning (Mojang changed scheme)
    (775, &["26.1", "26.1.1"]),
    // 1.21.x series
    (774, &["1.21.11"]),
    (773, &["1.21.10"]),
    (772, &["1.21.9"]),
    (771, &["1.21.8"]),
    (770, &["1.21.7"]),
    (769, &["1.21.6"]),
    (768, &["1.21.5"]),
    (767, &["1.21.4", "1.21.3", "1.21.2", "1.21", "1.21.1"]),
    // 1.20.x series
    (766, &["1.20.6", "1.20.5"]),
    (765, &["1.20.4", "1.20.3"]),
    (764, &["1.20.2"]),
    (763, &["1.20.1", "1.20"]),
    // 1.19.x series
    (762, &["1.19.4"]),
    (761, &["1.19.3"]),
    (760, &["1.19.2", "1.19.1"]),
    (759, &["1.19"]),
    // 1.18.x series
    (758, &["1.18.2"]),
    (757, &["1.18.1", "1.18"]),
    // 1.17.x series
    (756, &["1.17.1"]),
    (755, &["1.17"]),
    // 1.16.x series
    (754, &["1.16.5", "1.16.4"]),
    (753, &["1.16.3"]),
    (751, &["1.16.2"]),
    (736, &["1.16.1"]),
    (735, &["1.16"]),
    // 1.15.x series
    (578, &["1.15.2"]),
    (575, &["1.15.1"]),
    (573, &["1.15"]),
    // 1.14.x series
    (498, &["1.14.4"]),
    (490, &["1.14.3"]),
    (485, &["1.14.2"]),
    (480, &["1.14.1"]),
    (477, &["1.14"]),
    // 1.13.x series
    (404, &["1.13.2"]),
    (401, &["1.13.1"]),
    (393, &["1.13"]),
    // 1.12.x series
    (340, &["1.12.2"]),
    (338, &["1.12.1"]),
    (335, &["1.12"]),
    // 1.11.x series
    (316, &["1.11.2", "1.11.1"]),
    (315, &["1.11"]),
    // 1.10.x series (all share protocol 210)
    (210, &["1.10.2", "1.10.1", "1.10"]),
    // 1.9.x series
    (110, &["1.9.4", "1.9.3"]),
    (109, &["1.9.2"]),
    (108, &["1.9.1"]),
    (107, &["1.9"]),
    // 1.8.x series (all share protocol 47)
    (
        47,
        &[
            "1.8.9", "1.8.8", "1.8.7", "1.8.6", "1.8.5", "1.8.4", "1.8.3", "1.8.2", "1.8.1", "1.8",
        ],
    ),
    // 1.7.x series
    (5, &["1.7.10", "1.7.9", "1.7.8", "1.7.7", "1.7.6"]),
    (4, &["1.7.5", "1.7.4", "1.7.3", "1.7.2"]),
    // Legacy (pre-Netty, rarely seen)
    (0, &["1.6", "1.5", "1.4", "legacy"]),
];

/// Regex patterns for extracting version from disconnect messages.
static OUTDATED_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:outdated\s*server!?\s*(?:I'?m|I am)\s*still\s*on\s*|Incompatible\s*client!?\s*Please\s*use\s*|Please\s*use\s*Minecraft\s*)([0-9]+(?:\.[0-9]+)+(?:\.[0-9]+)*)").unwrap()
});

/// Try to extract a version string from a disconnect reason and map it to a protocol version.
///
/// Parses messages like:
/// - "Outdated server! I'm still on 1.20.1"
/// - "Incompatible client! Please use Minecraft 1.19.4"
/// - "Please use version 1.18.2"
///
/// Returns the protocol version if successfully parsed and mapped.
pub fn extract_protocol_from_disconnect(reason: &str) -> Option<i32> {
    let captured = OUTDATED_PATTERN.captures(reason)?;
    let version_str = captured.get(1)?.as_str();

    tracing::debug!(
        "Extracted version '{}' from disconnect reason: {}",
        version_str,
        reason
    );

    version_to_protocol(version_str)
}

/// Convert a version string like "1.20.1" or "1.19.4" to a protocol version.
///
/// Does fuzzy matching against known version patterns.
/// Handles both exact versions ("1.20.1") and embedded versions ("Paper 1.20.1").
pub fn version_to_protocol(version: &str) -> Option<i32> {
    let normalized = version.trim().to_lowercase();

    // Try exact match first
    for &(proto, patterns) in VERSION_PROTOCOL_MAP {
        for &pattern in patterns {
            if pattern == normalized {
                return Some(proto);
            }
        }
    }

    // Try fuzzy match: look for known version patterns within the string
    // e.g., "Paper 1.20.1" contains "1.20.1"
    let mut best_match: Option<(i32, usize)> = None;

    for &(proto, patterns) in VERSION_PROTOCOL_MAP {
        for &pattern in patterns {
            // Check if the string contains the pattern (embedded version)
            if normalized.contains(pattern) {
                let match_len = pattern.len();
                match best_match {
                    Some((_, best_len)) if match_len > best_len => {
                        best_match = Some((proto, match_len));
                    }
                    None => {
                        best_match = Some((proto, match_len));
                    }
                    _ => {}
                }
            }
        }
    }

    best_match.map(|(proto, _)| proto)
}

/// Result of a login attempt.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub obstacle: LoginObstacle,
    pub disconnect_reason: Option<String>,
    pub protocol_used: i32,
    pub latency_ms: u128,
}

/// Classification of the login outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginObstacle {
    /// Login succeeded — server accepts offline accounts.
    Success,
    /// Server sent Encryption Request — online-mode only.
    Premium,
    /// Disconnect reason mentions whitelist.
    Whitelist,
    /// Disconnect reason mentions banned.
    Banned,
    /// Server rejected for other reason.
    Rejected,
    /// Connection failed or timed out.
    Unreachable,
    /// Login attempt timed out.
    Timeout,
}

impl std::fmt::Display for LoginObstacle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginObstacle::Success => write!(f, "success"),
            LoginObstacle::Premium => write!(f, "premium"),
            LoginObstacle::Whitelist => write!(f, "whitelist"),
            LoginObstacle::Banned => write!(f, "banned"),
            LoginObstacle::Rejected => write!(f, "rejected"),
            LoginObstacle::Unreachable => write!(f, "unreachable"),
            LoginObstacle::Timeout => write!(f, "timeout"),
        }
    }
}

/// Write a VarInt to the buffer.
fn write_varint(buf: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut part = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            part |= 0x80;
        }
        buf.push(part);
        if value == 0 {
            break;
        }
    }
}

/// Read a VarInt from an async reader.
async fn read_varint<R: tokio::io::AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<u32> {
    let mut result = 0u32;
    let mut shift = 0;
    loop {
        let byte = reader.read_u8().await?;
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 35 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "VarInt too long",
            ));
        }
    }
    Ok(result)
}

/// Write a String (length-prefixed VarInt + UTF-8 bytes).
fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_varint(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
}

/// Build a Handshake packet with next_state = 2 (Login).
fn build_handshake_login(host: &str, port: u16, protocol_version: i32) -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, 0); // Packet ID
    write_varint(&mut packet, protocol_version as u32);
    write_string(&mut packet, host);
    packet.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut packet, 2); // Next State: 2 = Login

    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);
    final_packet
}

/// Build a Login Start packet. Version-aware.
///
/// - 1.7–1.12 (protocol ≤ 340): Name only
/// - 1.13–1.19.2 (protocol 393–760): Name + Has UUID (bool) + Optional UUID
/// - 1.19.3+ (protocol ≥ 761): Name + UUID (required)
fn build_login_start(username: &str, protocol_version: i32) -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, 0x00); // Packet ID: Login Start = 0x00
    write_string(&mut packet, username);

    if protocol_version >= 761 {
        // 1.19.3+: UUID as 16 bytes (required)
        let uuid = offline_uuid(username);
        packet.extend_from_slice(uuid.as_bytes());
    } else if protocol_version >= 393 {
        // 1.13–1.19.2: Has UUID = false (we don't provide UUID)
        packet.push(0x00); // false
    }
    // 1.7–1.12: no UUID field

    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);
    final_packet
}

/// Generate the offline UUID for a username.
/// Minecraft uses UUID v3 (MD5) of "OfflinePlayer:<name>".
fn offline_uuid(username: &str) -> uuid::Uuid {
    let name = format!("OfflinePlayer:{}", username);
    uuid::Uuid::new_v3(&uuid::Uuid::NAMESPACE_DNS, name.as_bytes())
}

/// Attempt an offline login to a server.
///
/// Uses the server's reported protocol version from SLP for maximum compatibility.
pub async fn attempt_login(addr: SocketAddr, protocol_version: i32) -> LoginResult {
    let start = std::time::Instant::now();

    let result = timeout(Duration::from_secs(10), async {
        attempt_login_inner(addr, protocol_version).await
    })
    .await;

    let latency = start.elapsed().as_millis();

    match result {
        Ok(r) => r,
        Err(_) => LoginResult {
            obstacle: LoginObstacle::Timeout,
            disconnect_reason: None,
            protocol_used: protocol_version,
            latency_ms: latency,
        },
    }
}

async fn attempt_login_inner(addr: SocketAddr, protocol_version: i32) -> LoginResult {
    // Connect
    let mut stream = match timeout(Duration::from_secs(5), TcpStream::connect(addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return LoginResult {
                obstacle: LoginObstacle::Unreachable,
                disconnect_reason: Some(e.to_string()),
                protocol_used: protocol_version,
                latency_ms: 0,
            };
        }
        Err(_) => {
            return LoginResult {
                obstacle: LoginObstacle::Unreachable,
                disconnect_reason: Some("connect timeout".to_string()),
                protocol_used: protocol_version,
                latency_ms: 0,
            };
        }
    };

    let _ = stream.set_nodelay(true);

    // Send Handshake (next_state = 2 for Login)
    let ip_str = addr.ip().to_string();
    let handshake = build_handshake_login(&ip_str, addr.port(), protocol_version);
    if let Err(e) = stream.write_all(&handshake).await {
        return LoginResult {
            obstacle: LoginObstacle::Unreachable,
            disconnect_reason: Some(format!("handshake write failed: {}", e)),
            protocol_used: protocol_version,
            latency_ms: 0,
        };
    }

    // Send Login Start
    let login_start = build_login_start(OFFLINE_USERNAME, protocol_version);
    if let Err(e) = stream.write_all(&login_start).await {
        return LoginResult {
            obstacle: LoginObstacle::Unreachable,
            disconnect_reason: Some(format!("login start write failed: {}", e)),
            protocol_used: protocol_version,
            latency_ms: 0,
        };
    }

    // Read response
    let mut reader = tokio::io::BufReader::with_capacity(4096, stream);

    // Read packet length
    let packet_len = match read_varint(&mut reader).await {
        Ok(len) => len as usize,
        Err(e) => {
            return LoginResult {
                obstacle: LoginObstacle::Unreachable,
                disconnect_reason: Some(format!("failed to read packet length: {}", e)),
                protocol_used: protocol_version,
                latency_ms: 0,
            };
        }
    };

    if packet_len > MAX_PACKET_SIZE {
        return LoginResult {
            obstacle: LoginObstacle::Rejected,
            disconnect_reason: Some(format!("packet too large: {} bytes", packet_len)),
            protocol_used: protocol_version,
            latency_ms: 0,
        };
    }

    // Read packet ID
    let packet_id = match read_varint(&mut reader).await {
        Ok(id) => id,
        Err(e) => {
            return LoginResult {
                obstacle: LoginObstacle::Unreachable,
                disconnect_reason: Some(format!("failed to read packet ID: {}", e)),
                protocol_used: protocol_version,
                latency_ms: 0,
            };
        }
    };

    match packet_id {
        0x00 => {
            // Disconnect — read reason (JSON Text Component)
            let reason = read_disconnect_reason(&mut reader).await;
            let obstacle = classify_disconnect(&reason);
            LoginResult {
                obstacle,
                disconnect_reason: Some(reason),
                protocol_used: protocol_version,
                latency_ms: 0,
            }
        }
        0x01 => {
            // Encryption Request → online-mode only
            LoginResult {
                obstacle: LoginObstacle::Premium,
                disconnect_reason: None,
                protocol_used: protocol_version,
                latency_ms: 0,
            }
        }
        0x02 => {
            // Login Success → offline mode works
            // We immediately disconnect (drop the stream)
            LoginResult {
                obstacle: LoginObstacle::Success,
                disconnect_reason: None,
                protocol_used: protocol_version,
                latency_ms: 0,
            }
        }
        0x03 => {
            // Set Compression — read threshold, then expect Login Success
            match read_varint(&mut reader).await {
                Ok(_threshold) => {
                    // After Set Compression, server sends Login Success
                    match read_varint(&mut reader).await {
                        Ok(len) => {
                            let mut reader2 = reader;
                            let mut buf = vec![0u8; len as usize];
                            let _ = reader2.read_exact(&mut buf).await;
                        }
                        Err(_) => {}
                    }
                    LoginResult {
                        obstacle: LoginObstacle::Success,
                        disconnect_reason: None,
                        protocol_used: protocol_version,
                        latency_ms: 0,
                    }
                }
                Err(_) => LoginResult {
                    obstacle: LoginObstacle::Rejected,
                    disconnect_reason: Some("failed to read compression threshold".to_string()),
                    protocol_used: protocol_version,
                    latency_ms: 0,
                },
            }
        }
        0x04 => {
            // Login Plugin Request (1.13+) — server wants a plugin response
            // We can't handle this, classify as modded/rejected
            LoginResult {
                obstacle: LoginObstacle::Rejected,
                disconnect_reason: Some("server requires plugin negotiation".to_string()),
                protocol_used: protocol_version,
                latency_ms: 0,
            }
        }
        other => LoginResult {
            obstacle: LoginObstacle::Rejected,
            disconnect_reason: Some(format!("unexpected packet ID: 0x{:02X}", other)),
            protocol_used: protocol_version,
            latency_ms: 0,
        },
    }
}

/// Read a Disconnect packet's reason field (JSON Text Component).
async fn read_disconnect_reason<R: tokio::io::AsyncReadExt + Unpin>(reader: &mut R) -> String {
    // Read the reason string (length-prefixed)
    match read_varint(reader).await {
        Ok(len) => {
            if len as usize > MAX_PACKET_SIZE {
                return format!("[reason too long: {} bytes]", len);
            }
            let mut buf = vec![0u8; len as usize];
            match reader.read_exact(&mut buf).await {
                Ok(_) => String::from_utf8_lossy(&buf).to_string(),
                Err(e) => format!("[read error: {}]", e),
            }
        }
        Err(e) => format!("[varint error: {}]", e),
    }
}

/// Classify a disconnect reason string into an obstacle type.
fn classify_disconnect(reason: &str) -> LoginObstacle {
    let lower = reason.to_lowercase();

    if lower.contains("whitelist") || lower.contains("white list") || lower.contains("allowlist") {
        return LoginObstacle::Whitelist;
    }

    if lower.contains("banned") || lower.contains("ban") {
        return LoginObstacle::Banned;
    }

    LoginObstacle::Rejected
}

/// Attempt login with protocol version extraction from disconnect messages.
///
/// First tries the provided protocol version.
/// If the server rejects with a version mismatch message (e.g., "Outdated server! I'm still on 1.20.1"),
/// extracts the version, maps it to a protocol version, and retries once.
/// If that also fails, returns the second result.
///
/// Returns the successful or final failed result.
pub async fn attempt_login_smart(addr: SocketAddr, protocol_version: i32) -> LoginResult {
    let result = attempt_login(addr, protocol_version).await;

    // If rejected, check if it's a version mismatch we can recover from
    if result.obstacle == LoginObstacle::Rejected {
        if let Some(ref reason) = result.disconnect_reason {
            if let Some(extracted_protocol) = extract_protocol_from_disconnect(reason) {
                if extracted_protocol != protocol_version {
                    tracing::debug!(
                        "Protocol mismatch detected for {}: extracted protocol {} from '{}', retrying...",
                        addr, extracted_protocol, reason
                    );
                    let retry_result = attempt_login(addr, extracted_protocol).await;

                    // Log the retry outcome
                    if retry_result.obstacle == LoginObstacle::Success {
                        tracing::info!(
                            "Login SUCCEEDED for {} after protocol correction: {} → {}",
                            addr,
                            protocol_version,
                            extracted_protocol
                        );
                    } else {
                        tracing::debug!(
                            "Login still failed for {} after protocol retry ({} → {}): {:?} - {}",
                            addr,
                            protocol_version,
                            extracted_protocol,
                            retry_result.obstacle,
                            retry_result
                                .disconnect_reason
                                .as_deref()
                                .unwrap_or("no reason")
                        );
                    }

                    return retry_result;
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_uuid() {
        let uuid = offline_uuid("NMCScan");
        // Should be deterministic
        let uuid2 = offline_uuid("NMCScan");
        assert_eq!(uuid, uuid2);

        // Different name should give different UUID
        let uuid3 = offline_uuid("Other");
        assert_ne!(uuid, uuid3);
    }

    #[test]
    fn test_build_login_start_old() {
        let packet = build_login_start("NMCScan", 47); // 1.8
        assert!(!packet.is_empty());
        // Packet: [packet_len][packet_id(0x00)][name_len(0x07)][name_bytes("NMCScan")]
        // Offset 3 = start of name bytes
        let payload = &packet[3..];
        assert_eq!(payload, b"NMCScan");
    }

    #[test]
    fn test_build_login_start_modern() {
        let packet = build_login_start("NMCScan", 775); // 26.1
        assert!(!packet.is_empty());
        // Should be longer than old version (includes UUID)
        let old_packet = build_login_start("NMCScan", 47);
        assert!(packet.len() > old_packet.len());
    }

    #[test]
    fn test_classify_disconnect() {
        assert_eq!(
            classify_disconnect("You are not whitelisted"),
            LoginObstacle::Whitelist
        );
        assert_eq!(
            classify_disconnect("You are banned from this server"),
            LoginObstacle::Banned
        );
        assert_eq!(
            classify_disconnect("Connection throttled!"),
            LoginObstacle::Rejected
        );
        assert_eq!(
            classify_disconnect("You have been allowlisted"),
            LoginObstacle::Whitelist
        );
    }

    #[test]
    fn test_version_to_protocol_exact() {
        assert_eq!(version_to_protocol("26.1"), Some(775));
        assert_eq!(version_to_protocol("26.1.1"), Some(775));
        assert_eq!(version_to_protocol("1.21.11"), Some(774));
        assert_eq!(version_to_protocol("1.21"), Some(767));
        assert_eq!(version_to_protocol("1.20.1"), Some(763));
        assert_eq!(version_to_protocol("1.20.2"), Some(764));
        assert_eq!(version_to_protocol("1.19.4"), Some(762));
        assert_eq!(version_to_protocol("1.16.5"), Some(754));
        assert_eq!(version_to_protocol("1.16.1"), Some(736));
        assert_eq!(version_to_protocol("1.8.9"), Some(47));
        assert_eq!(version_to_protocol("1.7.10"), Some(5));
        assert_eq!(version_to_protocol("1.7.2"), Some(4));
    }

    #[test]
    fn test_version_to_protocol_fuzzy() {
        // "Paper 1.20.1" should match "1.20.1" → 763
        assert_eq!(version_to_protocol("Paper 1.20.1"), Some(763));
        // "Spigot 1.19.4" should match "1.19.4" → 762
        assert_eq!(version_to_protocol("Spigot 1.19.4"), Some(762));
        // "1.8" partial match → 47 (all 1.8.x share protocol 47)
        assert_eq!(version_to_protocol("1.8"), Some(47));
        // "1.18.2" → 758
        assert_eq!(version_to_protocol("1.18.2"), Some(758));
        // "Fabric 1.16.5" → 754
        assert_eq!(version_to_protocol("Fabric 1.16.5"), Some(754));
    }

    #[test]
    fn test_extract_protocol_from_disconnect() {
        // Standard "Outdated server" messages
        assert_eq!(
            extract_protocol_from_disconnect("Outdated server! I'm still on 1.20.1"),
            Some(763)
        );
        assert_eq!(
            extract_protocol_from_disconnect("Outdated server! I'm still on 1.19.4"),
            Some(762)
        );
        assert_eq!(
            extract_protocol_from_disconnect("Outdated server! I am still on 1.16.5"),
            Some(754)
        );

        // "Incompatible client" messages
        assert_eq!(
            extract_protocol_from_disconnect("Incompatible client! Please use 1.21.4"),
            Some(767)
        );
        assert_eq!(
            extract_protocol_from_disconnect("Incompatible client! Please use Minecraft 1.20.4"),
            Some(765)
        );

        // No version info → None
        assert_eq!(
            extract_protocol_from_disconnect("You are not whitelisted"),
            None
        );
        assert_eq!(
            extract_protocol_from_disconnect("Connection throttled!"),
            None
        );
    }

    #[test]
    fn test_version_to_protocol_unknown() {
        // Version not in map → None
        assert_eq!(version_to_protocol("99.99.99"), None);
    }

    #[test]
    fn test_version_protocol_map_consistency() {
        // Verify that all protocol numbers are positive and patterns are non-empty
        for &(proto, patterns) in VERSION_PROTOCOL_MAP {
            assert!(proto >= 0, "Protocol {} should be non-negative", proto);
            assert!(
                !patterns.is_empty(),
                "Patterns for protocol {} should not be empty",
                proto
            );
            for &pattern in patterns {
                assert!(
                    !pattern.is_empty(),
                    "Pattern should not be empty for protocol {}",
                    proto
                );
            }
        }
    }
}
