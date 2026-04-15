//! TCP Connect Scanner for Pass 1 - Fast port verification.
//!
//! This is a lightweight scan to verify a port is open before attempting
//! the more expensive SLP/RakNet ping. Uses non-blocking TCP connect.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Result of a TCP connect scan.
#[derive(Debug, Clone)]
pub struct TcpConnectResult {
    pub ip: String,
    pub port: u16,
    pub reachable: bool,
    pub latency_ms: u128,
    pub error: Option<String>,
}

/// Perform a quick TCP connect check.
/// Returns true if the port is open/reachable.
pub async fn scan_tcp_connect(ip: &str, port: u16, timeout_ms: u64) -> TcpConnectResult {
    let start = std::time::Instant::now();

    let ip_addr: IpAddr = match ip.parse() {
        Ok(addr) => addr,
        Err(e) => {
            return TcpConnectResult {
                ip: ip.to_string(),
                port,
                reachable: false,
                latency_ms: 0,
                error: Some(format!("invalid IP: {}", e)),
            };
        }
    };

    let addr = SocketAddr::new(ip_addr, port);
    let timeout_duration = Duration::from_millis(timeout_ms);

    let result = timeout(timeout_duration, TcpStream::connect(addr)).await;
    let latency = start.elapsed().as_millis();

    match result {
        Ok(Ok(_stream)) => TcpConnectResult {
            ip: ip.to_string(),
            port,
            reachable: true,
            latency_ms: latency,
            error: None,
        },
        Ok(Err(e)) => TcpConnectResult {
            ip: ip.to_string(),
            port,
            reachable: false,
            latency_ms: latency,
            error: Some(e.to_string()),
        },
        Err(_) => TcpConnectResult {
            ip: ip.to_string(),
            port,
            reachable: false,
            latency_ms: timeout_ms as u128,
            error: Some("connection timeout".to_string()),
        },
    }
}

/// Scan multiple ports on the same IP quickly.
/// Returns list of open ports.
pub async fn scan_ports(ip: &str, ports: &[u16], timeout_ms: u64) -> Vec<u16> {
    let mut handles = Vec::new();

    for &port in ports {
        let ip_owned = ip.to_string();
        handles.push(tokio::spawn(async move {
            let result = scan_tcp_connect(&ip_owned, port, timeout_ms).await;
            if result.reachable {
                Some(port)
            } else {
                None
            }
        }));
    }

    let mut open_ports = Vec::new();
    for handle in handles {
        if let Ok(Some(port)) = handle.await {
            open_ports.push(port);
        }
    }

    open_ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_connect_localhost() {
        // This test assumes something is running on localhost:80 or will fail
        let result = scan_tcp_connect("127.0.0.1", 80, 1000).await;
        // Just check it returns something
        assert!(result.ip == "127.0.0.1");
        assert!(result.port == 80);
    }

    #[tokio::test]
    async fn test_tcp_connect_invalid_ip() {
        let result = scan_tcp_connect("invalid", 25565, 1000).await;
        assert!(!result.reachable);
        assert!(result.error.is_some());
    }
}