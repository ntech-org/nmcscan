//! Login queue service for testing offline login on active servers.
//!
//! Iterates through online servers, attempts offline login with "NMCScan",
//! and stores the result (obstacle type) directly on the server record.
//! Rate-limited to ~60 attempts/second with low concurrency.
//!
//! On startup, waits 30 minutes before processing servers that were never login-tested.
//! This gives the scanner time to do its initial SLP pass and populate version data.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{self, Duration};

use crate::network::login::{self, LoginObstacle, LoginResult, LATEST_PROTOCOL};
use crate::repositories::ServerRepository;
use chrono::{Duration as ChronoDuration, Utc};

/// Login queue statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginQueueStats {
    pub running: bool,
    pub total_attempts: u64,
    pub success: u64,
    pub premium: u64,
    pub whitelist: u64,
    pub banned: u64,
    pub rejected: u64,
    pub unreachable: u64,
    pub timeout: u64,
    pub last_server: Option<String>,
}

/// Login queue service.
pub struct LoginQueue {
    server_repo: Arc<ServerRepository>,
    running: Arc<AtomicBool>,
    stats: Arc<Mutex<LoginQueueStats>>,
    total_attempts: Arc<AtomicU64>,
    semaphore: Arc<Semaphore>,
}

impl LoginQueue {
    pub fn new(server_repo: Arc<ServerRepository>) -> Self {
        Self {
            server_repo,
            running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(Mutex::new(LoginQueueStats {
                running: false,
                total_attempts: 0,
                success: 0,
                premium: 0,
                whitelist: 0,
                banned: 0,
                rejected: 0,
                unreachable: 0,
                timeout: 0,
                last_server: None,
            })),
            total_attempts: Arc::new(AtomicU64::new(0)),
            semaphore: Arc::new(Semaphore::new(20)), // Max 20 concurrent login attempts
        }
    }

    /// Start the login queue background loop.
    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            tracing::warn!("Login queue is already running");
            return;
        }

        let queue = Arc::clone(self);
        tokio::spawn(async move {
            queue.run_loop().await;
        });

        // Update stats
        let stats = Arc::clone(&self.stats);
        tokio::spawn(async move {
            let mut s = stats.lock().await;
            s.running = true;
        });

        tracing::info!("Login queue started (60/sec rate limit, 20 max concurrent)");
    }

    /// Stop the login queue.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("Login queue stopping...");
    }

    /// Check if the queue is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get current statistics.
    pub async fn get_stats(&self) -> LoginQueueStats {
        let mut stats = self.stats.lock().await;
        stats.running = self.running.load(Ordering::Relaxed);
        stats.total_attempts = self.total_attempts.load(Ordering::Relaxed);
        stats.clone()
    }

    /// Manually trigger a login attempt to a specific server.
    pub async fn login_single(&self, ip: &str, port: u16) -> LoginResult {
        let addr: SocketAddr = match format!("{}:{}", ip, port).parse() {
            Ok(a) => a,
            Err(e) => {
                return LoginResult {
                    obstacle: LoginObstacle::Unreachable,
                    disconnect_reason: Some(format!("invalid address: {}", e)),
                    protocol_used: 0,
                    latency_ms: 0,
                };
            }
        };

        // Use latest protocol version for manual attempts
        let result = login::attempt_login(addr, 775).await;
        let obstacle_str = result.obstacle.to_string();
        let port: i16 = port.try_into().unwrap_or(25565);

        // Store result
        if let Err(e) = self
            .server_repo
            .update_login_result(ip, port as i32, &obstacle_str)
            .await
        {
            tracing::error!("Failed to update login result for {}:{}: {}", ip, port, e);
        }

        self.record_result(&result).await;
        result
    }

    /// Main loop: continuously fetch online servers and attempt login.
    async fn run_loop(&self) {
        tracing::info!("Login queue loop started");

        // Give the scanner 30 minutes to do initial SLP pass before we start login testing.
        // This ensures servers have version data populated from SLP first.
        let mut initial_delay_done = false;
        const INITIAL_DELAY_SECONDS: i64 = 1800; // 30 minutes

        // Token bucket: refill every 16.6ms for ~60/sec rate
        let mut interval = time::interval(Duration::from_micros(16667));
        let mut server_index = 0usize;

        while self.running.load(Ordering::Relaxed) {
            interval.tick().await;

            // Check initial delay on first iteration
            if !initial_delay_done {
                // Check if any server has been login-tested recently (within last 30 min)
                // If so, the initial delay is already satisfied by previous run
                let servers = match self.server_repo.get_online_servers(10).await {
                    Ok(s) => s,
                    Err(_) => {
                        time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                let has_recent_login = servers.iter().any(|s| {
                    s.last_login_at
                        .map(|lt| {
                            (Utc::now().naive_utc() - lt).num_seconds() < INITIAL_DELAY_SECONDS
                        })
                        .unwrap_or(false)
                });

                let has_never_tested = servers.iter().all(|s| s.last_login_at.is_none());

                if has_recent_login {
                    tracing::info!("Recent login activity detected, skipping initial delay");
                    initial_delay_done = true;
                } else if has_never_tested {
                    tracing::info!(
                        "No servers have been login-tested yet. Waiting {} minutes before starting...",
                        INITIAL_DELAY_SECONDS / 60
                    );
                    time::sleep(Duration::from_secs(INITIAL_DELAY_SECONDS as u64)).await;
                    tracing::info!("Initial delay complete, starting login queue");
                    initial_delay_done = true;
                    continue;
                } else {
                    // Mix of tested and untested servers - proceed normally
                    initial_delay_done = true;
                }
            }

            // Fetch a batch of online servers
            let servers = match self.server_repo.get_online_servers(500).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to fetch online servers: {}", e);
                    time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            if servers.is_empty() {
                time::sleep(Duration::from_secs(10)).await;
                continue;
            }

            // Cycle through servers
            if server_index >= servers.len() {
                server_index = 0;
            }

            let server = &servers[server_index];
            server_index += 1;

            // Skip if recently tested (within 1 hour)
            if let Some(last_login) = server.last_login_at {
                let elapsed = chrono::Utc::now().naive_utc() - last_login;
                if elapsed.num_seconds() < 3600 {
                    continue;
                }
            }

            let ip = server.ip.clone();
            let port = server.port;
            let server_version = server.version.clone(); // SLP-reported version string

            // Acquire concurrency permit
            let permit = match self.semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => continue,
            };

            let server_repo = Arc::clone(&self.server_repo);
            let stats = Arc::clone(&self.stats);
            let total_attempts = Arc::clone(&self.total_attempts);

            tokio::spawn(async move {
                let addr = SocketAddr::new(ip.ip(), port as u16);

                // Extract protocol version from SLP-reported version string if available
                let protocol = server_version
                    .as_ref()
                    .and_then(|v| login::version_to_protocol(v))
                    .unwrap_or(LATEST_PROTOCOL);

                // Use smart login that extracts version from disconnect messages
                let result = login::attempt_login_smart(addr, protocol).await;
                let obstacle_str = result.obstacle.to_string();

                // Update server record
                if let Err(e) = server_repo
                    .update_login_result(&ip.to_string(), port, &obstacle_str)
                    .await
                {
                    tracing::error!("Failed to update login result for {}:{}: {}", ip, port, e);
                }

                total_attempts.fetch_add(1, Ordering::Relaxed);

                // Update stats
                {
                    let mut s = stats.lock().await;
                    match result.obstacle {
                        LoginObstacle::Success => s.success += 1,
                        LoginObstacle::Premium => s.premium += 1,
                        LoginObstacle::Whitelist => s.whitelist += 1,
                        LoginObstacle::Banned => s.banned += 1,
                        LoginObstacle::Rejected => s.rejected += 1,
                        LoginObstacle::Unreachable => s.unreachable += 1,
                        LoginObstacle::Timeout => s.timeout += 1,
                    }
                    s.last_server = Some(format!("{}:{}", ip, port));
                }

                if result.obstacle == LoginObstacle::Success {
                    tracing::info!("Login SUCCESS: {}:{}", ip, port);
                } else {
                    tracing::debug!(
                        "Login {}: {}:{} - {} (version: {}, protocol: {})",
                        obstacle_str,
                        ip,
                        port,
                        result.disconnect_reason.as_deref().unwrap_or("no reason"),
                        server_version.as_deref().unwrap_or("unknown"),
                        protocol
                    );
                }

                drop(permit);
            });
        }

        // Mark as stopped
        {
            let mut s = self.stats.lock().await;
            s.running = false;
        }
        tracing::info!("Login queue loop stopped");
    }

    /// Record a login result in stats.
    async fn record_result(&self, result: &LoginResult) {
        self.total_attempts.fetch_add(1, Ordering::Relaxed);
        let mut s = self.stats.lock().await;
        match result.obstacle {
            LoginObstacle::Success => s.success += 1,
            LoginObstacle::Premium => s.premium += 1,
            LoginObstacle::Whitelist => s.whitelist += 1,
            LoginObstacle::Banned => s.banned += 1,
            LoginObstacle::Rejected => s.rejected += 1,
            LoginObstacle::Unreachable => s.unreachable += 1,
            LoginObstacle::Timeout => s.timeout += 1,
        }
    }
}
