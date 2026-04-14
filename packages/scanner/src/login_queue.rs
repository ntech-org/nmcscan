//! Login queue service for testing offline login on active servers.
//!
//! Iterates through online servers, attempts offline login with "NMCScan",
//! and stores the result (obstacle type) directly on the server record.
//! Rate-limited to ~60 attempts/second with low concurrency.
//!
//! DB writes are batched (50 results or 2s) to reduce pool pressure.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{self, Duration};

use chrono::Utc;
use nmcscan_shared::network::login::{self, LATEST_PROTOCOL, LoginObstacle, LoginResult};
use nmcscan_shared::repositories::ServerRepository;
use sea_orm::prelude::IpNetwork;

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

/// A pending login result to be batched.
struct PendingLoginResult {
    ip: String,
    port: i16,
    obstacle: String,
    should_update_last_seen: bool,
}

/// Login queue service.
pub struct LoginQueue {
    server_repo: Arc<ServerRepository>,
    running: Arc<AtomicBool>,
    stats: Arc<Mutex<LoginQueueStats>>,
    total_attempts: Arc<AtomicU64>,
    semaphore: Arc<Semaphore>,
}

#[allow(dead_code)]
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
            semaphore: Arc::new(Semaphore::new(20)),
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

        let stats = Arc::clone(&self.stats);
        tokio::spawn(async move {
            let mut s = stats.lock().await;
            s.running = true;
        });

        tracing::info!("Login queue started (60/sec rate limit, 20 max concurrent)");
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("Login queue stopping...");
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub async fn get_stats(&self) -> LoginQueueStats {
        let mut stats = self.stats.lock().await;
        stats.running = self.running.load(Ordering::Relaxed);
        stats.total_attempts = self.total_attempts.load(Ordering::Relaxed);
        stats.clone()
    }

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

        let result = login::attempt_login(addr, 775).await;
        let obstacle_str = result.obstacle.to_string();
        let port: i16 = port.try_into().unwrap_or(25565);

        if let Err(e) = self
            .server_repo
            .update_login_result(ip, port, &obstacle_str)
            .await
        {
            tracing::error!("Failed to update login result for {}:{}: {}", ip, port, e);
        }

        self.record_result(&result).await;
        result
    }

    /// Main loop with batched DB writes.
    async fn run_loop(&self) {
        tracing::info!("Login queue loop started");

        let mut initial_delay_done = false;
        const INITIAL_DELAY_SECONDS: i64 = 1800;

        let mut interval = time::interval(Duration::from_micros(16667));

        // Cursor-based pagination
        let mut cursor_ip: Option<IpNetwork> = None;
        let mut cursor_port: Option<i16> = None;
        let mut server_index = 0usize;
        let mut current_batch: Vec<nmcscan_shared::models::entities::servers::Model> = Vec::new();

        // Batch DB write channel
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PendingLoginResult>(200);
        let repo = Arc::clone(&self.server_repo);

        tokio::spawn(async move {
            let mut results = Vec::with_capacity(50);
            let mut last_seen_updates = Vec::with_capacity(50);
            let mut flush_interval = time::interval(Duration::from_secs(2));

            loop {
                tokio::select! {
                    Some(pending) = rx.recv() => {
                        let should_update = pending.should_update_last_seen;
                        let ip = pending.ip.clone();
                        let port = pending.port;
                        results.push((pending.ip, pending.port, pending.obstacle));
                        if should_update {
                            last_seen_updates.push((ip, port));
                        }
                        if results.len() >= 50 {
                            flush_login_batch(&repo, &mut results, &mut last_seen_updates).await;
                        }
                    }
                    _ = flush_interval.tick() => {
                        if !results.is_empty() {
                            flush_login_batch(&repo, &mut results, &mut last_seen_updates).await;
                        }
                    }
                    else => break,
                }
            }
        });

        while self.running.load(Ordering::Relaxed) {
            interval.tick().await;

            // Check initial delay
            if !initial_delay_done {
                let servers = match self.server_repo.get_online_servers(10).await {
                    Ok(s) => s,
                    Err(_) => {
                        time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                let has_recent_login = servers.iter().any(|s| {
                    s.last_login_at
                        .map(|lt| (Utc::now().naive_utc() - lt).num_seconds() < INITIAL_DELAY_SECONDS)
                        .unwrap_or(false)
                });

                let has_never_tested = servers.iter().all(|s| s.last_login_at.is_none());

                if has_recent_login {
                    tracing::info!("Recent login activity detected, skipping initial delay");
                    initial_delay_done = true;
                } else if has_never_tested {
                    tracing::info!(
                        "No servers have been login-tested yet. Waiting {} minutes...",
                        INITIAL_DELAY_SECONDS / 60
                    );
                    time::sleep(Duration::from_secs(INITIAL_DELAY_SECONDS as u64)).await;
                    tracing::info!("Initial delay complete, starting login queue");
                    initial_delay_done = true;
                    continue;
                } else {
                    initial_delay_done = true;
                }
            }

            // Fetch next batch
            if server_index >= current_batch.len() {
                server_index = 0;
                const MAX_LOGIN_AGE_HOURS: i64 = 48;
                match self
                    .server_repo
                    .get_online_servers_cursor_recent(500, cursor_ip, cursor_port, MAX_LOGIN_AGE_HOURS)
                    .await
                {
                    Ok(batch) => {
                        if batch.is_empty() {
                            cursor_ip = None;
                            cursor_port = None;
                            match self
                                .server_repo
                                .get_online_servers_cursor_recent(500, None, None, MAX_LOGIN_AGE_HOURS)
                                .await
                            {
                                Ok(b) => current_batch = b,
                                Err(e) => {
                                    tracing::error!("Failed to fetch online servers: {}", e);
                                    time::sleep(Duration::from_secs(5)).await;
                                    continue;
                                }
                            }
                        } else {
                            current_batch = batch;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch online servers: {}", e);
                        time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                }

                if current_batch.is_empty() {
                    time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            }

            let server = &current_batch[server_index];

            cursor_ip = Some(server.ip.clone());
            cursor_port = Some(server.port);
            server_index += 1;

            // Skip if recently tested
            if let Some(last_login) = server.last_login_at {
                let elapsed = chrono::Utc::now().naive_utc() - last_login;
                if elapsed.num_seconds() < 3600 {
                    continue;
                }
            }

            let ip = server.ip.clone();
            let port = server.port;
            let server_version = server.version.clone();

            let permit = match self.semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => continue,
            };

            let tx = tx.clone();
            let stats = Arc::clone(&self.stats);
            let total_attempts = Arc::clone(&self.total_attempts);

            tokio::spawn(async move {
                let addr = SocketAddr::new(ip.ip(), port as u16);

                let protocol = server_version
                    .as_ref()
                    .and_then(|v| login::version_to_protocol(v))
                    .unwrap_or(LATEST_PROTOCOL);

                let result = login::attempt_login_smart(addr, protocol).await;
                let obstacle_str = result.obstacle.to_string();

                let should_update_last_seen = result.obstacle != LoginObstacle::Timeout
                    && result.obstacle != LoginObstacle::Unreachable;

                // Send to batch writer instead of direct DB write
                let _ = tx.send(PendingLoginResult {
                    ip: ip.to_string(),
                    port: port as i16,
                    obstacle: obstacle_str,
                    should_update_last_seen,
                }).await;

                total_attempts.fetch_add(1, Ordering::Relaxed);

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
                }

                drop(permit);
            });
        }

        {
            let mut s = self.stats.lock().await;
            s.running = false;
        }
        tracing::info!("Login queue loop stopped");
    }

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

/// Flush a batch of login results to the DB.
async fn flush_login_batch(
    repo: &ServerRepository,
    results: &mut Vec<(String, i16, String)>,
    last_seen_updates: &mut Vec<(String, i16)>,
) {
    if !results.is_empty() {
        if let Err(e) = repo.batch_update_login_results(results.split_off(0)).await {
            tracing::error!("Failed to batch update login results: {}", e);
        }
    }
    if !last_seen_updates.is_empty() {
        if let Err(e) = repo.batch_update_last_seen(last_seen_updates.split_off(0).as_slice()).await {
            tracing::error!("Failed to batch update last_seen: {}", e);
        }
    }
}
