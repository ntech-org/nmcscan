//! Minecraft Bedrock Edition RakNet Unconnected Ping Protocol.
//!
//! Implements the RakNet unconnected ping and pong parsing.

use crate::network::slp::{Players, ServerStatus, Version};
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

// The standard RakNet magic bytes
const MAGIC: [u8; 16] = [
    0x00, 0xff, 0xff, 0x00, 0xfe, 0xfe, 0xfe, 0xfe, 0xfd, 0xfd, 0xfd, 0xfd, 0x12, 0x34, 0x56, 0x78,
];

pub async fn ping_server(
    addr: SocketAddr,
) -> Result<ServerStatus, Box<dyn std::error::Error + Send + Sync>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    // 0x01 = Unconnected Ping
    // Time (8 bytes)
    // Magic (16 bytes)
    // Client GUID (8 bytes)
    let mut packet = vec![0x01];

    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    packet.extend_from_slice(&time.to_be_bytes());
    packet.extend_from_slice(&MAGIC);

    let guid: u64 = rand::random();
    packet.extend_from_slice(&guid.to_be_bytes());

    socket.send_to(&packet, addr).await?;

    let mut buf = [0u8; 2048];
    // Fast 3-second timeout for the response
    let (len, _) = timeout(Duration::from_secs(3), socket.recv_from(&mut buf)).await??;

    if len < 35 || buf[0] != 0x1C {
        return Err("Invalid or unrecognized response".into());
    }

    // 0x1C (1 byte)
    // Time (8 bytes)
    // Server GUID (8 bytes)
    // Magic (16 bytes)
    // Server ID length (2 bytes)
    let server_id_len = u16::from_be_bytes([buf[33], buf[34]]) as usize;

    if len < 35 + server_id_len {
        return Err("Response too short".into());
    }

    let server_id_str = String::from_utf8_lossy(&buf[35..35 + server_id_len]).to_string();

    // Format: MCPE;MOTD;Protocol;Version;Players;Max;GUID;LevelName;GameMode;GameModeID;PortV4;PortV6;
    let parts: Vec<&str> = server_id_str.split(';').collect();

    if parts.len() < 6 {
        return Err("Invalid Server ID string format".into());
    }

    let motd = parts[1].to_string();
    let protocol: i32 = parts[2].parse().unwrap_or(0);
    let version_name = parts[3].to_string();
    let online: i32 = parts[4].parse().unwrap_or(0);
    let max: i32 = parts[5].parse().unwrap_or(0);

    Ok(ServerStatus {
        description: json!({"text": motd}),
        players: Some(Players {
            online,
            max,
            sample: None,
        }),
        version: Some(Version {
            name: version_name,
            protocol,
        }),
        favicon: None,
        ping: None,
        mod_info: None,
    })
}
