//! Minecraft Server List Ping (SLP) protocol implementation.
//!
//! Implements the standard handshake and status request packets manually
//! using VarInt encoding. Supports both modern (1.7+) and legacy (1.6) SLP.
//!
//! Protocol: https://wiki.vg/Server_List_Ping

use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::net::SocketAddr;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

const MAX_PACKET_SIZE: usize = 256 * 1024; // 256KB limit for safety

#[derive(Error, Debug)]
pub enum SlpError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Connection timeout")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("Invalid response format: {0}")]
    InvalidResponse(String),
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[allow(dead_code)]
    #[error("Server returned an error: {0}")]
    ServerError(String),
}

/// Server status response from SLP.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerStatus {
    pub description: serde_json::Value,
    pub players: Option<Players>,
    pub version: Option<Version>,
    pub favicon: Option<String>,
    #[serde(default)]
    pub ping: Option<u128>, // Latency in ms
    #[serde(rename = "modinfo")]
    pub mod_info: Option<ModInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModInfo {
    #[serde(rename = "type")]
    pub mod_type: String,
    #[serde(rename = "modList", default)]
    pub mod_list: Option<Vec<ModEntry>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModEntry {
    pub modid: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Players {
    pub online: i32,
    pub max: i32,
    pub sample: Option<Vec<PlayerSample>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlayerSample {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Version {
    pub name: String,
    pub protocol: i32,
}

/// Write a VarInt to the buffer.
pub fn write_varint(buf: &mut Vec<u8>, mut value: u32) {
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

/// Read a VarInt from a reader.
#[allow(dead_code)]
pub fn read_varint<R: Read>(reader: &mut R) -> io::Result<u32> {
    let mut result = 0u32;
    let mut shift = 0;

    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let byte = byte[0];

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

/// Build a Handshake packet.
pub fn build_handshake(host: &str, port: u16, protocol_version: i32) -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, 0); // Packet ID
    write_varint(&mut packet, protocol_version as u32);

    let host_bytes = host.as_bytes();
    write_varint(&mut packet, host_bytes.len() as u32);
    packet.extend_from_slice(host_bytes);

    packet.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut packet, 1); // Next State (1 = Status)

    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);
    final_packet
}

/// Build a Status Request packet.
pub fn build_status_request() -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, 0); // Packet ID

    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);
    final_packet
}

/// Build a Ping packet.
pub fn build_ping(payload: i64) -> Vec<u8> {
    let mut packet = Vec::new();
    write_varint(&mut packet, 1); // Packet ID 0x01 for Ping
    packet.extend_from_slice(&payload.to_be_bytes());

    let mut final_packet = Vec::new();
    write_varint(&mut final_packet, packet.len() as u32);
    final_packet.extend(packet);
    final_packet
}

/// Perform a Server List Ping and return server status.
pub async fn ping_server(
    addr: SocketAddr,
    hostname: Option<&str>,
) -> Result<ServerStatus, SlpError> {
    // 10 second overall timeout for the entire operation
    timeout(Duration::from_secs(10), async {
        // Try Modern SLP first
        match ping_server_modern(addr, hostname).await {
            Ok(status) => Ok(status),
            Err(e) => {
                tracing::debug!("Modern SLP failed for {}: {}, trying legacy...", addr, e);
                ping_server_legacy(addr).await
            }
        }
    })
    .await?
}

async fn ping_server_modern(
    addr: SocketAddr,
    hostname: Option<&str>,
) -> Result<ServerStatus, SlpError> {
    let start_time = std::time::Instant::now();
    // 5 second connect timeout
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(addr)).await??;
    stream.set_nodelay(true)?;

    // Handshake & Status Request
    let ip_str = addr.ip().to_string();
    let handshake_host = hostname.unwrap_or(&ip_str);
    let handshake = build_handshake(handshake_host, addr.port(), 47);
    stream.write_all(&handshake).await?;

    let status_request = build_status_request();
    stream.write_all(&status_request).await?;

    // Use a buffered reader for efficient VarInt parsing
    let mut reader = tokio::io::BufReader::with_capacity(4096, stream);

    // Helper to read a VarInt from tokio BufReader
    async fn read_varint_async<R: tokio::io::AsyncReadExt + Unpin>(
        reader: &mut R,
    ) -> io::Result<u32> {
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

    // 1. Read Packet Length
    let _packet_len = read_varint_async(&mut reader).await? as usize;

    // 2. Read Packet ID
    let packet_id = read_varint_async(&mut reader).await?;
    if packet_id != 0 {
        return Err(SlpError::InvalidResponse(format!(
            "Expected packet ID 0, got {}",
            packet_id
        )));
    }

    // 3. Read JSON Length
    let json_len = read_varint_async(&mut reader).await? as usize;
    if json_len > MAX_PACKET_SIZE {
        return Err(SlpError::InvalidResponse(format!(
            "JSON too large: {}",
            json_len
        )));
    }

    // 4. Read JSON bytes
    let mut json_bytes = vec![0u8; json_len];
    reader.read_exact(&mut json_bytes).await?;

    let mut response: ServerStatus = serde_json::from_slice(&json_bytes)?;

    // Optional Ping phase
    let ping_payload = 12345678i64;
    let ping_packet = build_ping(ping_payload);
    let mut stream = reader.into_inner();
    let _ = stream.write_all(&ping_packet).await;

    // Try to read pong briefly (2s for high-latency)
    let mut pong_buf = [0u8; 12];
    if let Ok(Ok(n)) = timeout(Duration::from_secs(2), stream.read(&mut pong_buf)).await {
        if n >= 2 {
            response.ping = Some(start_time.elapsed().as_millis());
        }
    }

    if response.ping.is_none() {
        response.ping = Some(start_time.elapsed().as_millis());
    }

    Ok(response)
}

/// Legacy SLP (1.6 and below) support.
async fn ping_server_legacy(addr: SocketAddr) -> Result<ServerStatus, SlpError> {
    // 5 second overall timeout for legacy
    timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(addr).await?;

        // Send 0xFE 0x01 (1.4-1.6)
        stream.write_all(&[0xFE, 0x01]).await?;

        // Response starts with 0xFF
        let mut header = [0u8; 1];
        stream.read_exact(&mut header).await?;
        if header[0] != 0xFF {
            return Err(SlpError::InvalidResponse(format!(
                "Legacy: expected 0xFF, got 0x{:02X}",
                header[0]
            )));
        }

        // Next is 2 bytes of length (short, number of characters)
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf).await?;
        let char_count = u16::from_be_bytes(len_buf) as usize;

        if char_count > 1000 {
            return Err(SlpError::InvalidResponse(
                "Legacy: response too long".to_owned(),
            ));
        }

        // Read string (UTF-16BE)
        let mut string_bytes = vec![0u8; char_count * 2];
        stream.read_exact(&mut string_bytes).await?;

        let utf16_chars: Vec<u16> = string_bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();

        let response_str = String::from_utf16(&utf16_chars)
            .map_err(|_| SlpError::InvalidResponse("Legacy: invalid UTF-16".to_owned()))?;

        // Modern Legacy (1.6) starts with §1\0
        if response_str.starts_with("§1") {
            let parts: Vec<&str> = response_str.split('\0').collect();
            if parts.len() >= 6 {
                return Ok(ServerStatus {
                    description: serde_json::Value::String(parts[3].to_string()),
                    players: Some(Players {
                        online: parts[4].parse().unwrap_or(0),
                        max: parts[5].parse().unwrap_or(0),
                        sample: None,
                    }),
                    version: Some(Version {
                        name: parts[2].to_string(),
                        protocol: parts[1].parse().unwrap_or(0),
                    }),
                    favicon: None,
                    ping: None,
                    mod_info: None,
                });
            }
        }

        // Old Legacy (pre-1.6) format: MOTD§Online§Max
        let parts: Vec<&str> = response_str.split('§').collect();
        if parts.len() >= 3 {
            Ok(ServerStatus {
                description: serde_json::Value::String(parts[0].to_string()),
                players: Some(Players {
                    online: parts[1].parse().unwrap_or(0),
                    max: parts[2].parse().unwrap_or(0),
                    sample: None,
                }),
                version: Some(Version {
                    name: "Legacy".to_string(),
                    protocol: 0,
                }),
                favicon: None,
                ping: None,
                mod_info: None,
            })
        } else {
            Err(SlpError::InvalidResponse(
                "Legacy: malformed response".to_owned(),
            ))
        }
    })
    .await?
}

/// Extract MOTD text from description, handling complex JSON recursive structures.
pub fn extract_motd(status: &ServerStatus) -> String {
    parse_json_text(&status.description)
}

/// Extract server brand from SLP status.
///
/// Classification:
/// - "Vanilla" = vanilla-like (accepts vanilla clients): Paper, Spigot, Purpur, Bukkit, Arclight, Pufferfish
/// - "Forge" = requires Forge client mods
/// - "Fabric" = requires Fabric client mods
/// - "NeoForge" = requires NeoForge client mods
/// - "Proxy" = Velocity, BungeeCord, Waterfall
/// - "Vanilla" = default fallback
pub fn extract_brand(status: &ServerStatus) -> String {
    // 1. Check modinfo field (Forge/FML servers include this in SLP)
    if let Some(mod_info) = &status.mod_info {
        let t = mod_info.mod_type.to_lowercase();
        if t == "fml" || t == "forge" {
            return "Forge".to_string();
        }
        if !t.is_empty() {
            return capitalize_first(&mod_info.mod_type);
        }
    }

    // 2. Version string analysis
    if let Some(version) = &status.version {
        let name = version.name.to_lowercase();

        // Server-side only (accept vanilla clients) → classify as "Vanilla"
        // These are all vanilla-compatible server implementations
        if name.contains("paper") || name.contains("pufferfish") || name.contains("arclight") {
            return "Vanilla".to_string();
        }
        if name.contains("spigot") || name.contains("craftbukkit") || name.contains("purpur") {
            return "Vanilla".to_string();
        }

        // True modded (require client-side mods)
        if name.contains("forge") || name.contains("fml") {
            return "Forge".to_string();
        }
        if name.contains("fabric") {
            return "Fabric".to_string();
        }
        if name.contains("neoforge") {
            return "NeoForge".to_string();
        }

        // Proxies
        if name.contains("velocity") {
            return "Proxy".to_string();
        }
        if name.contains("bungeecord") || name.contains("bungee") {
            return "Proxy".to_string();
        }
        if name.contains("waterfall") {
            return "Proxy".to_string();
        }

        // Multi-version strings indicate a proxy
        if name.contains(" - ") || name.contains(" to ") {
            return "Proxy".to_string();
        }
        if name.contains(',') && name.contains('.') {
            // "1.8, 1.9, 1.10" style version lists
            return "Proxy".to_string();
        }
    }

    "Vanilla".to_string()
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn parse_json_text(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => {
            let mut result = String::new();
            if let Some(serde_json::Value::String(text)) = obj.get("text") {
                result.push_str(text);
            }
            if let Some(serde_json::Value::Array(extra)) = obj.get("extra") {
                for item in extra {
                    result.push_str(&parse_json_text(item));
                }
            }
            result
        }
        serde_json::Value::Array(arr) => {
            let mut result = String::new();
            for item in arr {
                result.push_str(&parse_json_text(item));
            }
            result
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_varint_encode_decode_small() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 0);
        write_varint(&mut buf, 1);
        write_varint(&mut buf, 127);

        let mut cursor = Cursor::new(&buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 0);
        assert_eq!(read_varint(&mut cursor).unwrap(), 1);
        assert_eq!(read_varint(&mut cursor).unwrap(), 127);
    }

    #[test]
    fn test_varint_encode_decode_large() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 300);
        write_varint(&mut buf, 16383);

        let mut cursor = Cursor::new(&buf);
        assert_eq!(read_varint(&mut cursor).unwrap(), 300);
        assert_eq!(read_varint(&mut cursor).unwrap(), 16383);
    }

    #[test]
    fn test_slp_packet_build() {
        let packet = build_handshake("192.0.2.1", 25565, 47);
        assert!(!packet.is_empty());
    }
}
