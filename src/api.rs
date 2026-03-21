//! Axum web API for server monitoring.
//!
//! Endpoints:
//! - GET /health - Health check with server count
//! - GET /servers?limit=50&status=online - List servers
//! - GET /server/{ip} - Server details
//! - GET /stats - Scanner statistics (queues, ASN counts)
//! - GET /asns - List ASNs with server counts
//! - GET /asns/{asn} - ASN details with IP ranges
//! - GET /api/exclude - Current exclude list
//! - POST /api/scan/test - Trigger test scan
//! - GET / - Static HTML dashboard

use axum::{
    extract::{Path, Query, State, Request},
    http::{StatusCode, HeaderMap},
    middleware::{self, Next},
    response::{Html, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

use crate::db::{Database, Server};
use crate::scheduler::Scheduler;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub scheduler: Arc<Scheduler>,
    pub api_key: Option<String>,
}

/// Query parameters for /servers endpoint.
#[derive(Deserialize)]
pub struct ServerQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub status: Option<String>,
}

fn default_limit() -> i32 {
    50
}

/// Test scan request.
#[derive(Deserialize)]
pub struct TestScanRequest {
    /// Number of servers to scan (default: 10)
    pub count: Option<usize>,
    /// Region filter (us, eu, uk, au, br, asia)
    pub region: Option<String>,
    /// Quick test with 10 servers
    pub quick: Option<bool>,
}

/// Test scan response.
#[derive(Serialize)]
pub struct TestScanResponse {
    pub status: String,
    pub servers_added: usize,
    pub servers: Vec<TestServerInfo>,
}

#[derive(Serialize)]
pub struct TestServerInfo {
    pub ip: String,
    pub port: u16,
    pub name: String,
}

/// Health check response.
#[derive(serde::Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub total_servers: i64,
}

/// Scanner statistics.
#[derive(serde::Serialize)]
pub struct StatsResponse {
    pub total_servers: i64,
    pub online_servers: i64,
    pub total_players: i64,
    pub asn_hosting: i64,
    pub asn_residential: i64,
    pub asn_unknown: i64,
}

/// ASN record for API response.
#[derive(serde::Serialize)]
pub struct AsnResponse {
    pub asn: String,
    pub org: String,
    pub category: String,
    pub country: Option<String>,
    pub server_count: i64,
}

/// Exclude list entry.
#[derive(serde::Serialize)]
pub struct ExcludeEntry {
    pub network: String,
    pub comment: Option<String>,
}

/// Middleware to check for API key in headers.
async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(expected_key) = &state.api_key {
        let auth_header = headers.get("X-API-Key")
            .and_then(|h| h.to_str().ok());

        if auth_header != Some(expected_key) {
            tracing::warn!("Unauthorized access attempt from {:?}", request.uri());
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(request).await)
}

/// Create the Axum router with all endpoints.
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Protected API routes
    let api_routes = Router::new()
        .route("/stats", get(get_stats))
        .route("/servers", get(list_servers))
        .route("/server/:ip", get(get_server))
        .route("/asns", get(list_asns))
        .route("/asns/:asn", get(get_asn))
        .route("/exclude", get(get_exclude_list))
        .route("/scan/test", post(trigger_test_scan))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/health", get(health_check))
        .merge(api_routes)
        .route("/", get(dashboard))
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(state)
}

/// GET /health - Health check endpoint.
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let total_servers = state.db.get_server_count().await.unwrap_or(0);
    Json(HealthResponse {
        status: "ok".to_string(),
        total_servers,
    })
}

/// GET /stats - Get scanner statistics.
async fn get_stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let total_servers = state.db.get_server_count().await.unwrap_or(0);
    let online_servers = state.db.get_online_count().await.unwrap_or(0);
    let total_players = state.db.get_total_players().await.unwrap_or(0);

    let (asn_hosting, asn_residential, asn_unknown) =
        state.db.get_asn_stats().await.unwrap_or((0, 0, 0));

    Json(StatsResponse {
        total_servers,
        online_servers,
        total_players,
        asn_hosting,
        asn_residential,
        asn_unknown,
    })
}

/// GET /servers - List servers with optional filters.
async fn list_servers(
    State(state): State<AppState>,
    Query(query): Query<ServerQuery>,
) -> Json<Vec<Server>> {
    let servers = state
        .db
        .get_all_servers(query.status.as_deref(), query.limit)
        .await
        .unwrap_or_default();
    Json(servers)
}

/// GET /server/{ip} - Get server details.
async fn get_server(
    State(state): State<AppState>,
    Path(ip): Path<String>,
) -> Result<Json<Server>, StatusCode> {
    state
        .db
        .get_server(&ip)
        .await
        .unwrap()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /asns - List all ASNs.
async fn list_asns(State(state): State<AppState>) -> Json<Vec<AsnResponse>> {
    let asns = state.db.get_all_asns().await.unwrap_or_default();

    let responses: Vec<AsnResponse> = asns
        .into_iter()
        .map(|asn| AsnResponse {
            asn: asn.asn,
            org: asn.org,
            category: format!("{:?}", asn.category),
            country: asn.country,
            server_count: 0, // Would need to query servers by ASN
        })
        .collect();

    Json(responses)
}

/// GET /asns/{asn} - Get ASN details.
async fn get_asn(
    State(state): State<AppState>,
    Path(asn): Path<String>,
) -> Result<Json<AsnResponse>, StatusCode> {
    let all_asns = state.db.get_all_asns().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let asn_record = all_asns
        .into_iter()
        .find(|a| a.asn == asn)
        .ok_or(StatusCode::NOT_FOUND)?;

    // Count servers in this ASN's ranges
    let ranges = state.db.get_all_asn_ranges().await.unwrap_or_default();
    let server_count = ranges
        .iter()
        .filter(|r| r.asn == asn)
        .count() as i64;

    Ok(Json(AsnResponse {
        asn: asn_record.asn,
        org: asn_record.org,
        category: format!("{:?}", asn_record.category),
        country: asn_record.country,
        server_count,
    }))
}

/// GET /exclude - Get current exclude list.
async fn get_exclude_list() -> Json<Vec<ExcludeEntry>> {
    // Read exclude.conf and parse entries
    let content = std::fs::read_to_string("exclude.conf").unwrap_or_default();
    let entries: Vec<ExcludeEntry> = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            // Extract comment if present
            let (network, comment) = if let Some(idx) = line.find('#') {
                (line[..idx].trim(), Some(line[idx + 1..].trim()))
            } else {
                (line, None)
            };

            if network.is_empty() {
                return None;
            }

            Some(ExcludeEntry {
                network: network.to_string(),
                comment: comment.map(String::from),
            })
        })
        .collect();

    Json(entries)
}

/// GET / - Static HTML dashboard.
async fn dashboard() -> Html<&'static str> {
    Html(include_str!("../assets/index.html"))
}

/// POST /scan/test - Trigger a test scan with known servers.
async fn trigger_test_scan(
    State(state): State<AppState>,
    Json(payload): Json<TestScanRequest>,
) -> Json<TestScanResponse> {
    use crate::test_mode;
    use crate::asn::AsnCategory;

    let quick = payload.quick.unwrap_or(false);
    let count = payload.count.unwrap_or(10);
    
    let test_servers = if quick {
        test_mode::get_quick_test_servers()
    } else if let Some(region) = payload.region {
        let mut servers = test_mode::get_servers_by_region(&region);
        servers.truncate(count);
        servers
    } else {
        let servers: Vec<(String, u16, String, String)> = test_mode::KNOWN_MINECRAFT_SERVERS
            .iter()
            .take(count)
            .map(|(ip, port, name, host)| (ip.to_string(), *port, name.to_string(), host.to_string()))
            .collect();
        servers
    };

    // Add servers to scheduler
    for (ip, port, _name, host) in &test_servers {
        let mut target = crate::scheduler::ServerTarget::new(ip.clone(), *port);
        target.category = AsnCategory::Hosting;
        target.priority = 1; // Hot priority
        target.hostname = Some(host.clone());
        
        let _ = state.db.insert_server_if_new(ip, *port as i32).await;
        
        state.scheduler.add_server(target).await;
    }

    let servers_info: Vec<TestServerInfo> = test_servers
        .iter()
        .map(|(ip, port, name, _host)| TestServerInfo {
            ip: ip.clone(),
            port: *port,
            name: name.clone(),
        })
        .collect();

    tracing::info!("Test scan triggered: {} servers added", servers_info.len());

    Json(TestScanResponse {
        status: "ok".to_string(),
        servers_added: servers_info.len(),
        servers: servers_info,
    })
}

/// Start the web server.
pub async fn run_server(state: AppState, addr: &str) -> std::io::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Web API listening on {}", addr);
    axum::serve(listener, app).await
}
