//! Lightweight HTTP server for the scanner service.
//!
//! Exposes status and control endpoints that the API service calls
//! to get live queue stats and trigger test scans.

use crate::login_queue::LoginQueue;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use nmcscan_shared::services::scheduler::Scheduler;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct ScannerState {
    pub scheduler: Arc<Scheduler>,
    pub login_queue: Arc<LoginQueue>,
}

#[derive(Serialize)]
pub struct ScannerStatus {
    pub queues: QueueSizes,
    pub login_queue: LoginQueueStatus,
}

#[derive(Serialize)]
pub struct QueueSizes {
    pub hot: usize,
    pub warm: usize,
    pub cold: usize,
    pub discovery: usize,
}

#[derive(Serialize)]
pub struct LoginQueueStatus {
    pub running: bool,
    pub total_attempts: u64,
    pub success: u64,
    pub premium: u64,
    pub whitelist: u64,
    pub banned: u64,
    pub rejected: u64,
    pub unreachable: u64,
    pub timeout: u64,
}

#[derive(Deserialize)]
pub struct TestScanRequest {
    pub quick: Option<bool>,
    pub region: Option<String>,
    pub count: Option<usize>,
}

#[derive(Serialize)]
pub struct TestScanResponse {
    pub status: String,
    pub servers_added: usize,
}

#[derive(Deserialize)]
pub struct LoginTriggerRequest {
    pub ip: String,
    pub port: u16,
}

pub async fn run_http_server(
    state: ScannerState,
    listen_addr: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/status", get(get_status))
        .route("/scan/test", post(post_test_scan))
        .route("/login-queue/status", get(get_login_status))
        .route("/login-queue/trigger", post(post_login_trigger))
        .with_state(state);

    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!("Scanner HTTP server listening on {}", listen_addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_status(State(state): State<ScannerState>) -> Json<ScannerStatus> {
    let (hot, warm, cold, discovery) = state.scheduler.get_queue_sizes().await;
    let login_stats = state.login_queue.get_stats().await;

    Json(ScannerStatus {
        queues: QueueSizes {
            hot,
            warm,
            cold,
            discovery,
        },
        login_queue: LoginQueueStatus {
            running: login_stats.running,
            total_attempts: login_stats.total_attempts,
            success: login_stats.success,
            premium: login_stats.premium,
            whitelist: login_stats.whitelist,
            banned: login_stats.banned,
            rejected: login_stats.rejected,
            unreachable: login_stats.unreachable,
            timeout: login_stats.timeout,
        },
    })
}

async fn post_test_scan(
    State(state): State<ScannerState>,
    Json(payload): Json<TestScanRequest>,
) -> Json<TestScanResponse> {
    use nmcscan_shared::utils::test_mode;

    let quick = payload.quick.unwrap_or(false);
    let count = payload.count.unwrap_or(10);

    let test_servers: Vec<(String, u16, String, String)> = if quick {
        test_mode::get_quick_test_servers()
    } else if let Some(region) = payload.region {
        let mut servers = test_mode::get_servers_by_region(&region);
        servers.truncate(count);
        servers
    } else {
        let servers: Vec<(String, u16, String, String)> =
            test_mode::KNOWN_MINECRAFT_SERVERS
                .iter()
                .take(count)
                .map(|s| (s.0.to_string(), s.1, s.2.to_string(), s.3.to_string()))
                .collect();
        servers
    };

    let total = test_servers.len();
    tracing::info!("Test scan triggered: dispatching {} servers", total);

    // Add servers to the scheduler's hot queue
    for (ip, port, _name, _host) in test_servers {
        let server_type = if port == 19132 {
            "bedrock".to_string()
        } else {
            "java".to_string()
        };
        let mut target =
            nmcscan_shared::services::scheduler::ServerTarget::new(ip.clone(), port, server_type);
        target.priority = 1; // Hot queue
        target.next_scan_at = None; // Scan immediately
        state.scheduler.add_server(target, false).await;
    }

    Json(TestScanResponse {
        status: "dispatched".to_string(),
        servers_added: total,
    })
}

async fn get_login_status(State(state): State<ScannerState>) -> Json<LoginQueueStatus> {
    let stats = state.login_queue.get_stats().await;
    Json(LoginQueueStatus {
        running: stats.running,
        total_attempts: stats.total_attempts,
        success: stats.success,
        premium: stats.premium,
        whitelist: stats.whitelist,
        banned: stats.banned,
        rejected: stats.rejected,
        unreachable: stats.unreachable,
        timeout: stats.timeout,
    })
}

async fn post_login_trigger(
    State(state): State<ScannerState>,
    Json(payload): Json<LoginTriggerRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let result = state.login_queue.login_single(&payload.ip, payload.port).await;

    Ok(Json(serde_json::json!({
        "ip": payload.ip,
        "port": payload.port,
        "obstacle": result.obstacle.to_string(),
        "disconnect_reason": result.disconnect_reason,
        "protocol_used": result.protocol_used,
        "latency_ms": result.latency_ms,
    })))
}
