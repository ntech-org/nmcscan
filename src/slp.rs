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
pub async fn ping_server(addr: SocketAddr, hostname: Option<&str>) -> Result<ServerStatus, SlpError> {
    // Try Modern SLP first
    match ping_server_modern(addr, hostname).await {
        Ok(status) => Ok(status),
        Err(e) => {
            tracing::debug!("Modern SLP failed for {}: {}, trying legacy...", addr, e);
            ping_server_legacy(addr).await
        }
    }
}

async fn ping_server_modern(addr: SocketAddr, hostname: Option<&str>) -> Result<ServerStatus, SlpError> {
    let start_time = std::time::Instant::now();
    let mut stream = timeout(Duration::from_secs(3), TcpStream::connect(addr)).await??;
    stream.set_nodelay(true)?;

    // Handshake & Status Request
    let ip_str = addr.ip().to_string();
    let handshake_host = hostname.unwrap_or(&ip_str);
    let handshake = build_handshake(handshake_host, addr.port(), 47);
    stream.write_all(&handshake).await?;
    
    let status_request = build_status_request();
    stream.write_all(&status_request).await?;

    // Read response length
    let mut len_buf = [0u8; 5];
    let mut bytes_read = 0;
    loop {
        let n = stream.read(&mut len_buf[bytes_read..bytes_read+1]).await?;
        if n == 0 { return Err(SlpError::InvalidResponse("Connection closed while reading length".to_owned())); }
        bytes_read += 1;
        if len_buf[bytes_read - 1] & 0x80 == 0 { break; }
        if bytes_read >= 5 { return Err(SlpError::InvalidResponse("VarInt too long".to_owned())); }
    }

    let mut cursor = io::Cursor::new(&len_buf[..bytes_read]);
    let packet_len = read_varint(&mut cursor)? as usize;
    if packet_len > MAX_PACKET_SIZE {
        return Err(SlpError::InvalidResponse(format!("Packet too large: {}", packet_len)));
    }

    let mut packet_body = vec![0u8; packet_len];
    stream.read_exact(&mut packet_body).await?;

    let mut body_cursor = io::Cursor::new(packet_body);
    let packet_id = read_varint(&mut body_cursor)?;
    if packet_id != 0 {
        return Err(SlpError::InvalidResponse(format!("Expected packet ID 0, got {}", packet_id)));
    }

    let json_len = read_varint(&mut body_cursor)? as usize;
    if json_len > MAX_PACKET_SIZE {
        return Err(SlpError::InvalidResponse(format!("JSON too large: {}", json_len)));
    }
    
    let mut json_bytes = vec![0u8; json_len];
    io::Read::read_exact(&mut body_cursor, &mut json_bytes)?;
    
    let mut response: ServerStatus = serde_json::from_slice(&json_bytes)?;
    
    // Optional Ping phase to verify and measure latency
    let ping_payload = 12345678i64;
    let ping_packet = build_ping(ping_payload);
    let _ = stream.write_all(&ping_packet).await;
    
    // Try to read pong
    let mut pong_buf = [0u8; 12]; // Length(VarInt) + ID(VarInt) + Payload(8)
    if let Ok(Ok(n)) = timeout(Duration::from_millis(500), stream.read(&mut pong_buf)).await {
        if n >= 2 {
            // Success, we got a pong (or at least some data)
            response.ping = Some(start_time.elapsed().as_millis());
        }
    }

    if response.ping.is_none() {
        response.ping = Some(start_time.elapsed().as_millis());
    }

    Ok(response)
}

/// Legacy SLP (1.6) support.
async fn ping_server_legacy(addr: SocketAddr) -> Result<ServerStatus, SlpError> {
    let mut stream = timeout(Duration::from_secs(3), TcpStream::connect(addr)).await??;
    
    // Send 0xFE 0x01
    stream.write_all(&[0xFE, 0x01]).await?;
    
    // Response starts with 0xFF
    let mut header = [0u8; 1];
    stream.read_exact(&mut header).await?;
    if header[0] != 0xFF {
        return Err(SlpError::InvalidResponse(format!("Legacy: expected 0xFF, got 0x{:02X}", header[0])));
    }
    
    // Next is 2 bytes of length (short, number of characters)
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await?;
    let char_count = u16::from_be_bytes(len_buf) as usize;
    
    // Read string (UTF-16BE)
    let mut string_bytes = vec![0u8; char_count * 2];
    stream.read_exact(&mut string_bytes).await?;
    
    let utf16_chars: Vec<u16> = string_bytes
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    
    let response_str = String::from_utf16(&utf16_chars)
        .map_err(|_| SlpError::InvalidResponse("Legacy: invalid UTF-16".to_owned()))?;
    
    // Format: §1\0Protocol\0Version\0MOTD\0Online\0Max
    let parts: Vec<&str> = response_str.split('\0').collect();
    if parts.len() >= 6 {
        Ok(ServerStatus {
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
        })
    } else {
        Err(SlpError::InvalidResponse("Legacy: malformed response".to_owned()))
    }
}

/// Extract MOTD text from description, handling complex JSON recursive structures.
pub fn extract_motd(status: &ServerStatus) -> String {
    parse_json_text(&status.description)
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
