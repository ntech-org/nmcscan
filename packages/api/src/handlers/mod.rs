//! Axum web API for server monitoring.
//!
//! Endpoints:
//! - GET /health - Health check with server count
//! - GET /info - Contact information for public landing page
//! - GET /api/stats - Scanner statistics (queues, ASN counts)
//! - GET /api/servers - List servers with search and filtering
//! - GET /api/server/{ip} - Server details
//! - GET /api/server/{ip}/history - Historical player count
//! - GET /api/players - Search for a player
//! - GET /api/asns - List ASNs with server counts
//! - GET /api/asns/{asn} - ASN details with IP ranges
//! - GET /api/exclude - Current exclude list
//! - POST /api/exclude - Add new exclusion
//! - POST /api/scan/test - Trigger test scan
//! - GET / - Static dashboard (fallback to assets)

use axum::{
    Router,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Json, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u64,
    pub limit: u64,
}

#[derive(Serialize)]
pub struct ServerResponse {
    pub ip: String,
    pub port: i16,
    pub server_type: String,
    pub status: String,
    pub players_online: i32,
    pub players_max: i32,
    pub motd: Option<String>,
    pub version: Option<String>,
    pub priority: i32,
    pub last_seen: Option<chrono::NaiveDateTime>,
    pub consecutive_failures: i32,
    pub whitelist_prob: f64,
    pub asn: Option<String>,
    pub country: Option<String>,
    pub favicon: Option<String>,
    pub brand: Option<String>,
    pub asn_org: Option<String>,
    pub asn_tags: Vec<String>,
    pub login_obstacle: Option<String>,
    pub last_login_at: Option<chrono::NaiveDateTime>,
    pub flags: Vec<String>,
    pub created_at: Option<chrono::NaiveDateTime>,
}

use nmcscan_shared::models::entities::servers;
use nmcscan_shared::repositories::{
    ApiKeyRepository, AsnRepository, MinecraftAccountRepository, ServerRepository, StatsRepository,
};
// Login queue is now part of scanner service
use nmcscan_shared::utils::exclude::ExcludeManager;

pub mod api_keys;
pub mod minecraft_accounts;

/// Shared application state.
#[allow(dead_code)]
#[derive(Clone)]
pub struct AppState {
    pub db: sea_orm::DatabaseConnection,
    pub server_repo: Arc<ServerRepository>,
    pub asn_repo: Arc<AsnRepository>,
    pub stats_repo: Arc<StatsRepository>,
    pub api_key_repo: Arc<ApiKeyRepository>,
    pub minecraft_account_repo: Arc<MinecraftAccountRepository>,
    pub scheduler: Option<Arc<nmcscan_shared::services::scheduler::Scheduler>>,
    pub exclude_list: Arc<ExcludeManager>,
    pub api_key: Option<String>,
    pub contact_email: Option<String>,
    pub discord_link: Option<String>,
}

#[derive(Clone)]
pub struct AuthContext {
    pub user_id: Option<i32>,
    #[allow(dead_code)]
    pub is_master: bool,
}

#[derive(Serialize)]
pub struct ContactResponse {
    pub email: Option<String>,
    pub discord: Option<String>,
}

/// Query parameters for /servers endpoint.
#[derive(Deserialize)]
pub struct ServerQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub status: Option<String>,
    pub search: Option<String>,
    pub min_players: Option<i32>,
    pub max_players: Option<i32>,
    pub version: Option<String>,
    pub asn_category: Option<String>,
    pub whitelist_prob_min: Option<f64>,
    pub country: Option<String>,
    pub brand: Option<String>,
    pub server_type: Option<String>,
    pub login: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub cursor_players: Option<i32>,
    pub cursor_ip: Option<String>,
    pub cursor_last_seen: Option<chrono::NaiveDateTime>,
    pub cursor_created_at: Option<chrono::NaiveDateTime>,
    pub asn: Option<String>,
    pub min_max_players: Option<i32>,
    pub max_max_players: Option<i32>,
    pub flags: Option<String>,
}

fn default_limit() -> i32 {
    50
}

/// Query parameters for /players endpoint.
#[derive(Deserialize)]
pub struct PlayerQuery {
    pub name: String,
}

#[derive(Serialize)]
pub struct PlayerResponse {
    pub ip: String,
    pub port: i16,
    pub player_name: String,
    pub last_seen: chrono::NaiveDateTime,
}

#[derive(Serialize)]
pub struct ServerPlayerResponse {
    pub player_name: String,
    pub player_uuid: String,
    pub last_seen: chrono::NaiveDateTime,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub timestamp: chrono::NaiveDateTime,
    pub players_online: i32,
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

#[derive(Deserialize)]
pub struct AddExcludeRequest {
    pub network: String,
    pub comment: Option<String>,
}

/// Login queue trigger request.
#[derive(Deserialize)]
#[allow(dead_code)]
pub struct LoginTriggerRequest {
    pub ip: String,
    #[serde(default = "default_login_port")]
    pub port: u16,
}

#[allow(dead_code)]
fn default_login_port() -> u16 {
    25565
}

/// Login queue status response.
#[derive(Serialize)]
#[allow(dead_code)]
pub struct LoginQueueStatusResponse {
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

/// Login trigger response.
#[derive(Serialize)]
#[allow(dead_code)]
pub struct LoginTriggerResponse {
    pub obstacle: String,
    pub disconnect_reason: Option<String>,
    pub latency_ms: u128,
    pub protocol_used: i32,
}

/// Scan progress response.
#[derive(Serialize)]
pub struct ScanProgressResponse {
    pub categories: Vec<nmcscan_shared::repositories::asns::CategoryProgress>,
    pub queues: nmcscan_shared::services::scheduler::QueueStats,
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
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub total_servers: i64,
}

/// Scanner statistics.
#[derive(Serialize)]
pub struct StatsResponse {
    pub total_servers: i64,
    pub online_servers: i64,
    pub total_players: i64,
    pub asn_hosting: i64,
    pub asn_residential: i64,
    pub asn_unknown: i64,
}

/// ASN record for API response.
#[derive(Serialize)]
pub struct AsnResponse {
    pub asn: String,
    pub org: String,
    pub category: String,
    pub country: Option<String>,
    pub server_count: i64,
    pub tags: Vec<String>,
}

/// Exclude list entry.
#[derive(Serialize, Deserialize, Clone)]
pub struct ExcludeEntry {
    pub network: String,
    pub comment: Option<String>,
}

/// Middleware to check for API key in headers.
async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let mut authenticated_user_id = None;
    let mut is_master = false;

    if let Some(auth_header) = headers.get("X-API-Key").and_then(|h| h.to_str().ok()) {
        if state.api_key.as_deref() == Some(auth_header) {
            is_master = true;
            // Allow proxy to impersonate user if master key is used
            if let Some(user_id) = headers
                .get("X-User-Id")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<i32>().ok())
            {
                authenticated_user_id = Some(user_id);
            }
        } else {
            // Check api_keys table (hash the provided raw key first to compare)
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(auth_header.as_bytes());
            let hash_hex = format!("{:x}", hasher.finalize());

            if let Ok(Some(user_id)) = state.api_key_repo.validate_key(&hash_hex).await {
                authenticated_user_id = Some(user_id);
            }
        }
    }

    if !is_master && authenticated_user_id.is_none() {
        tracing::warn!("Unauthorized access attempt from {:?}", request.uri());
        return Err(StatusCode::UNAUTHORIZED);
    }

    request.extensions_mut().insert(AuthContext {
        user_id: authenticated_user_id,
        is_master,
    });

    Ok(next.run(request).await)
}

#[derive(Serialize)]
pub struct MinecraftAccountResponse {
    pub id: i32,
    pub email: String,
    pub status: String,
    pub expires_at: Option<chrono::NaiveDateTime>,
}

#[derive(Deserialize)]
pub struct CreateMinecraftAccountRequest {
    pub email: String,
    pub password: Option<String>,
    pub access_token: Option<String>,
}

/// Create the Axum router with all endpoints.
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::HeaderName::from_static("x-api-key"),
            axum::http::header::HeaderName::from_static("x-user-id"),
        ]);

    // Protected API routes
    let protected_routes = Router::new()
        .route("/stats", get(get_stats))
        .route("/servers", get(list_servers))
        .route("/server/{ip}", get(get_server))
        .route("/server/{ip}/history", get(get_server_history))
        .route("/server/{ip}/players", get(get_server_players))
        .route("/players", get(search_players))
        .route("/asns", get(list_asns))
        .route("/asns/{asn}", get(get_asn))
        .route("/exclude", get(get_exclude_list).post(add_exclusion))
        .route("/scan/test", post(trigger_test_scan))
        .route("/scan/progress", get(get_scan_progress))
        .route("/login-queue/status", get(login_queue_status))
        .route("/login-queue/start", post(login_queue_start))
        .route("/login-queue/stop", post(login_queue_stop))
        .route("/login-queue/trigger", post(login_queue_trigger))
        .route("/keys", get(api_keys::list_keys).post(api_keys::create_key))
        .route("/keys/{id}", axum::routing::delete(api_keys::revoke_key))
        .route(
            "/minecraft-accounts",
            get(minecraft_accounts::list_accounts).post(minecraft_accounts::add_account),
        )
        .route(
            "/minecraft-accounts/{id}",
            axum::routing::delete(minecraft_accounts::delete_account),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Combine all API routes under /api
    let api_routes = Router::new()
        .route("/health", get(health_check))
        .route("/info", get(get_info))
        .merge(protected_routes);

    Router::new()
        .nest("/api", api_routes)
        .layer(CompressionLayer::new())
        .layer(cors)
        .with_state(state)
}

/// GET /health - Health check endpoint.
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let total_servers = state
        .stats_repo
        .get_global_stats()
        .await
        .map(|(t, _, _)| t)
        .unwrap_or(0);
    Json(HealthResponse {
        status: "ok".to_string(),
        total_servers,
    })
}

/// GET /info - Get contact information for the public landing page.
async fn get_info(State(state): State<AppState>) -> Json<ContactResponse> {
    Json(ContactResponse {
        email: state.contact_email.clone(),
        discord: state.discord_link.clone(),
    })
}

/// GET /api/stats - Get scanner statistics.
async fn get_stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let (total_servers, online_servers, total_players) = state
        .stats_repo
        .get_global_stats()
        .await
        .unwrap_or((0, 0, 0));
    let (asn_hosting, asn_residential, _, asn_unknown) = state
        .asn_repo
        .get_asn_stats_counts()
        .await
        .unwrap_or((0, 0, 0, 0));

    Json(StatsResponse {
        total_servers,
        online_servers,
        total_players,
        asn_hosting,
        asn_residential,
        asn_unknown,
    })
}

/// GET /api/servers - List servers with optional filters.
async fn list_servers(
    State(state): State<AppState>,
    Query(query): Query<ServerQuery>,
) -> Json<Vec<ServerResponse>> {
    // Parse DSL tokens from the search parameter if present
    let parsed = query
        .search
        .as_deref()
        .map(nmcscan_shared::utils::query_parser::parse);

    // DSL values act as defaults; explicit query params override them
    let status = query
        .status
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.status.as_deref()));
    // Use free_text from DSL parser (falls back to full search when no DSL tokens found)
    let search_text = parsed.as_ref().and_then(|p| p.free_text.as_deref());
    let min_players = query
        .min_players
        .or(parsed.as_ref().and_then(|p| p.min_players));
    let max_players = query
        .max_players
        .or(parsed.as_ref().and_then(|p| p.max_players));
    let version = query
        .version
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.version.as_deref()));
    let asn_category = query
        .asn_category
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.asn_category.as_deref()));
    let country = query
        .country
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.country.as_deref()));
    let brand = query
        .brand
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.brand.as_deref()));
    let server_type = query
        .server_type
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.server_type.as_deref()));
    let asn = query
        .asn
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.asn.as_deref()));
    let min_max_players = query
        .min_max_players
        .or(parsed.as_ref().and_then(|p| p.min_max_players));
    let max_max_players = query
        .max_max_players
        .or(parsed.as_ref().and_then(|p| p.max_max_players));

    // Support both DSL flags and explicit flag query param
    let mut flags_filter = parsed.as_ref().map(|p| p.flags.clone()).unwrap_or_default();
    if let Some(f) = &query.flags {
        flags_filter.extend(f.split(',').map(|s| s.to_string()));
    }

    // Explicit login query param overrides DSL-parsed login
    let login_obstacle = query
        .login
        .as_deref()
        .or(parsed.as_ref().and_then(|p| p.login.as_deref()));

    let servers = state
        .server_repo
        .get_all_servers(
            status,
            search_text,
            query.limit as u64,
            min_players,
            max_players,
            version,
            asn_category,
            query.whitelist_prob_min,
            country,
            brand,
            server_type,
            query.sort_by.as_deref(),
            query.sort_order.as_deref(),
            query.cursor_players,
            query.cursor_ip.as_deref(),
            query.cursor_last_seen,
            query.cursor_created_at,
            asn,
            min_max_players,
            max_max_players,
            flags_filter,
            login_obstacle,
        )
        .await
        .unwrap_or_default();

    let asns_list = state.asn_repo.get_all_asns().await.unwrap_or_default();

    let responses = servers
        .into_iter()
        .map(|s| {
            let mut asn_org = None;
            let mut asn_tags = Vec::new();

            if let Some(asn_num) = &s.asn {
                if let Some(asn) = asns_list.iter().find(|a| a.asn == *asn_num) {
                    asn_org = Some(asn.org.clone());
                    asn_tags = asn
                        .tags
                        .as_ref()
                        .map(|t| {
                            t.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();
                }
            }
            enrich_server_response(s, asn_org, asn_tags)
        })
        .collect();

    Json(responses)
}

/// GET /api/server/{ip} - Get server details.
async fn get_server(
    State(state): State<AppState>,
    Path(ip_param): Path<String>,
) -> Result<Json<ServerResponse>, StatusCode> {
    let (ip, port) = if let Some((i, p)) = ip_param.split_once(':') {
        (i, p.parse::<i16>().unwrap_or(25565))
    } else {
        (ip_param.as_str(), 25565)
    };

    let server = state
        .server_repo
        .get_server(ip, port)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut asn_org = None;
    let mut asn_tags = Vec::new();

    if let Some(asn_num) = &server.asn {
        if let Some(asn_list) = state.asn_repo.get_all_asns().await.ok() {
            if let Some(asn) = asn_list.iter().find(|a| a.asn == *asn_num) {
                asn_org = Some(asn.org.clone());
                asn_tags = asn
                    .tags
                    .as_ref()
                    .map(|t| {
                        t.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
            }
        }
    }

    Ok(Json(enrich_server_response(server, asn_org, asn_tags)))
}

fn enrich_server_response(
    server: servers::Model,
    asn_org: Option<String>,
    asn_tags: Vec<String>,
) -> ServerResponse {
    let flags = server
        .flags
        .as_deref()
        .map(|f| {
            f.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    ServerResponse {
        ip: server.ip.ip().to_string(),
        port: server.port as i16,
        server_type: server.server_type,
        status: server.status,
        players_online: server.players_online,
        players_max: server.players_max,
        motd: server.motd,
        version: server.version,
        priority: server.priority,
        last_seen: server.last_seen,
        consecutive_failures: server.consecutive_failures,
        whitelist_prob: server.whitelist_prob,
        asn: server.asn,
        country: server.country,
        favicon: server.favicon,
        brand: server.brand,
        asn_org,
        asn_tags,
        login_obstacle: server.login_obstacle,
        last_login_at: server.last_login_at,
        flags,
        created_at: server.created_at,
    }
}

/// GET /api/server/{ip}/history - Get historical player count.
async fn get_server_history(
    State(state): State<AppState>,
    Path(ip_param): Path<String>,
) -> Json<Vec<HistoryResponse>> {
    let (ip, port) = if let Some((i, p)) = ip_param.split_once(':') {
        (i, p.parse::<i16>().unwrap_or(25565))
    } else {
        (ip_param.as_str(), 25565)
    };

    let history = state
        .server_repo
        .get_server_history(ip, port, 100)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|h| HistoryResponse {
            timestamp: h.timestamp,
            players_online: h.players_online,
        })
        .collect();
    Json(history)
}

/// GET /api/server/{ip}/players - Get players seen on a server.
async fn get_server_players(
    State(state): State<AppState>,
    Path(ip_param): Path<String>,
) -> Json<Vec<ServerPlayerResponse>> {
    let (ip, port) = if let Some((i, p)) = ip_param.split_once(':') {
        (i, p.parse::<i16>().unwrap_or(25565))
    } else {
        (ip_param.as_str(), 25565)
    };

    let players = state
        .server_repo
        .get_server_players(ip, port)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| ServerPlayerResponse {
            player_name: p.player_name,
            player_uuid: p.player_uuid.unwrap_or_default(),
            last_seen: p.last_seen,
        })
        .collect();
    Json(players)
}

/// GET /api/players - Search for a player.
async fn search_players(
    State(state): State<AppState>,
    Query(query): Query<PlayerQuery>,
) -> Json<Vec<PlayerResponse>> {
    if query.name.len() < 3 {
        return Json(vec![]);
    }
    let players = state
        .server_repo
        .search_players(&query.name)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| PlayerResponse {
            ip: p.ip.ip().to_string(),
            port: p.port as i16,
            player_name: p.player_name,
            last_seen: p.last_seen,
        })
        .collect();
    Json(players)
}

/// GET /api/asns - List all ASNs.
async fn list_asns(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Json<PaginatedResponse<AsnResponse>> {
    let page = query.page.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);

    let (asns, total) = state
        .asn_repo
        .get_asn_list_paginated(page, limit)
        .await
        .unwrap_or_else(|_| (Vec::new(), 0));

    let responses: Vec<AsnResponse> = asns
        .into_iter()
        .map(|asn| AsnResponse {
            asn: asn.asn,
            org: asn.org,
            category: asn.category,
            country: asn.country,
            server_count: asn.server_count,
            tags: asn
                .tags
                .map(|t| {
                    t.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();

    Json(PaginatedResponse {
        items: responses,
        total: total as i64,
        page,
        limit,
    })
}

/// GET /api/asns/{asn} - Get ASN details.
async fn get_asn(
    State(state): State<AppState>,
    Path(asn_num): Path<String>,
) -> Result<Json<AsnResponse>, StatusCode> {
    let all_asns = state
        .asn_repo
        .get_all_asns()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all_stats = state
        .asn_repo
        .get_asn_list_with_counts()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(asn) = all_asns.into_iter().find(|a| a.asn == asn_num) {
        let stats = all_stats.into_iter().find(|s| s.asn == asn_num);
        Ok(Json(AsnResponse {
            asn: asn.asn,
            org: asn.org,
            category: asn.category,
            country: asn.country,
            server_count: stats.map(|s| s.server_count).unwrap_or(0),
            tags: asn
                .tags
                .map(|t| {
                    t.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// GET /api/exclude - Get current exclude list.
async fn get_exclude_list(
    Query(query): Query<PaginationQuery>,
) -> Json<PaginatedResponse<ExcludeEntry>> {
    let page = query.page.unwrap_or(0);
    let limit = query.limit.unwrap_or(50);

    let content = std::fs::read_to_string("exclude.conf").unwrap_or_default();
    let mut all_entries: Vec<ExcludeEntry> = content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

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

    all_entries.reverse();

    let total = all_entries.len() as i64;
    let start = (page * limit) as usize;
    let end = std::cmp::min(start + limit as usize, all_entries.len());

    let items = if start < all_entries.len() {
        all_entries[start..end].to_vec()
    } else {
        Vec::new()
    };

    Json(PaginatedResponse {
        items,
        total,
        page,
        limit,
    })
}

/// POST /api/exclude - Add a new exclusion.
async fn add_exclusion(
    State(state): State<AppState>,
    Json(payload): Json<AddExcludeRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .exclude_list
        .add_exclusion(&payload.network, payload.comment.as_deref())
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    tracing::info!(
        "IP/Network {} excluded via dashboard: {:?}",
        payload.network,
        payload.comment
    );
    Ok(StatusCode::CREATED)
}

/// POST /api/scan/test - Trigger a test scan with known servers.
async fn trigger_test_scan(
    State(state): State<AppState>,
    Json(payload): Json<TestScanRequest>,
) -> Json<TestScanResponse> {
    use nmcscan_shared::utils::test_mode;

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
            .map(|(ip, port, name, host)| {
                (ip.to_string(), *port, name.to_string(), host.to_string())
            })
            .collect();
        servers
    };

    for (ip, port, _name, host) in &test_servers {
        let server_type = if *port == 19132 { "bedrock" } else { "java" };
        let mut target = nmcscan_shared::services::scheduler::ServerTarget::new(
            ip.clone(),
            *port,
            server_type.to_string(),
        );
        target.category = nmcscan_shared::models::asn::AsnCategory::Hosting;
        target.priority = 1;
        target.hostname = Some(host.clone());

        let port: i16 = (*port).try_into().unwrap_or(25565);
        let _ = state
            .server_repo
            .insert_server_if_new(ip, port, server_type)
            .await;
        if let Some(scheduler) = &state.scheduler {
            scheduler.add_server(target, true).await;
        }
    }

    let servers_info: Vec<TestServerInfo> = test_servers
        .into_iter()
        .map(|(ip, port, name, _host)| TestServerInfo { ip, port, name })
        .collect();

    Json(TestScanResponse {
        status: "ok".to_string(),
        servers_added: servers_info.len(),
        servers: servers_info,
    })
}

/// GET /api/login-queue/status - Get login queue status and statistics.
/// Note: Login queue is now part of the scanner service.
async fn login_queue_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "unavailable",
        "message": "Login queue is managed by the scanner service (nmcscan-scanner)"
    }))
}

/// POST /api/login-queue/start - Start the login queue.
/// Note: Login queue is now part of the scanner service.
async fn login_queue_start() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "unavailable",
        "message": "Login queue is managed by the scanner service (nmcscan-scanner)"
    }))
}

/// POST /api/login-queue/stop - Stop the login queue.
/// Note: Login queue is now part of the scanner service.
async fn login_queue_stop() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "unavailable",
        "message": "Login queue is managed by the scanner service (nmcscan-scanner)"
    }))
}

/// POST /api/login-queue/trigger - Manually trigger a login attempt.
/// Note: Login queue is now part of the scanner service.
async fn login_queue_trigger() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "unavailable",
        "message": "Login queue is managed by the scanner service (nmcscan-scanner)"
    }))
}

/// GET /api/scan/progress - Get scan cycle progress and queue stats.
async fn get_scan_progress(
    State(state): State<AppState>,
) -> Result<Json<ScanProgressResponse>, StatusCode> {
    let categories = state
        .asn_repo
        .get_scan_progress()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let queues = if let Some(scheduler) = &state.scheduler {
        scheduler.get_queue_stats().await
    } else {
        nmcscan_shared::services::scheduler::QueueStats {
            hot: 0,
            warm: 0,
            cold: 0,
            discovery: 0,
        }
    };
    Ok(Json(ScanProgressResponse { categories, queues }))
}

/// Start the web server.
pub async fn run_server(state: AppState, addr: &str) -> std::io::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Web API listening on {}", addr);
    axum::serve(listener, app).await
}
