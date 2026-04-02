//! NMCScan - High-performance Minecraft Server Scanner
//!
//! A safe, ethical scanner with priority-based scheduling and strict exclude list enforcement.
//!
//! # Safety Features
//! - Strict exclude.conf enforcement (US Military, Universities, complaining IPs)
//! - Rate limiting (~100 connections/sec)
//! - Concurrency limiting (max 200 simultaneous tasks)
//! - 3-second timeout per connection
//! - No authentication attempts, no exploit scanning
//!
//! # Scan Tiers
//! - **Hot**: Online servers, last seen < 4 hours - ran 2-4 times/day
//! - **Warm**: Known hosting ASN ranges, not scanned in 7 days - ran 2-3 times/week
//! - **Cold**: Residential IPs, high-failure servers - ran 1-2 times/month

mod handlers;
mod services;
mod models;
pub mod repositories;
mod network;
mod utils;

use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use sea_orm::{Database, ConnectOptions, EntityTrait, QueryFilter, ColumnTrait, QuerySelect};
use migration::Migrator;
use sea_orm_migration::MigratorTrait;
use crate::repositories::{ServerRepository, AsnRepository, StatsRepository, ApiKeyRepository};
use crate::services::asn_fetcher::AsnFetcher;
use crate::services::scheduler::{Scheduler, ServerTarget};
use crate::services::login_queue::LoginQueue;
use crate::network::scanner::Scanner;
use crate::utils::exclude::{ExcludeList, ExcludeManager};
use crate::utils::test_mode;
use crate::models::asn::AsnCategory;

use clap::Parser;

/// NMCScan - High-performance Minecraft Server Scanner
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Enable test mode (scan only known servers)
    #[arg(short, long, env = "TEST_MODE", default_value = "false")]
    test_mode: bool,

    /// Maximum servers to scan in test mode
    #[arg(long, env = "TEST_MAX_SERVERS", default_value = "50")]
    test_max_servers: usize,

    /// Quick test with 10 servers only
    #[arg(long, default_value = "false")]
    quick_test: bool,

    /// Scan interval in seconds for test mode
    #[arg(long, env = "TEST_SCAN_INTERVAL", default_value = "60")]
    test_interval: u64,

    /// Region filter for test servers (us, eu, uk, au, br, asia)
    #[arg(long, env = "TEST_REGIONS")]
    test_regions: Option<String>,

    /// Log level (debug, info, warn, error)
    #[arg(short, long, env = "RUST_LOG", default_value = "info")]
    log_level: String,

    /// API key for dashboard authentication (optional, disables auth if empty)
    #[arg(long, env = "API_KEY")]
    api_key: Option<String>,

    /// Contact email for public landing page
    #[arg(long, env = "CONTACT_EMAIL")]
    contact_email: Option<String>,

    /// Discord link for public landing page
    #[arg(long, env = "DISCORD_LINK")]
    discord_link: Option<String>,

    /// PostgreSQL database URL
    #[arg(short, long, env = "DATABASE_URL", default_value = "postgres://nmcscan:nmcscan_secret@localhost:5432/nmcscan")]
    database: String,

    /// Path to exclude.conf file
    #[arg(short, long, env = "EXCLUDE_FILE", default_value = "exclude.conf")]
    exclude_file: String,

    /// Target scans per second (Connections Per Second)
    #[arg(long, env = "TARGET_RPS", default_value = "100")]
    target_rps: u64,

    /// Target concurrent scan tasks
    #[arg(long, env = "TARGET_CONCURRENCY", default_value = "2500")]
    target_concurrency: u32,

    /// Target scans per second for cold/residential IPs
    #[arg(long, env = "TARGET_COLD_RPS")]
    target_cold_rps: Option<u64>,

    /// API listen address
    #[arg(long, env = "LISTEN_ADDR", default_value = "0.0.0.0:3000")]
    listen_addr: String,

    /// Force ASN database re-import from iptoasn.com on startup
    #[arg(long, default_value = "false")]
    force_asn_import: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file
    let _ = dotenvy::dotenv();
    
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or(args.log_level),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("🎮 Starting NMCScan...");
    
    if args.test_mode || args.quick_test {
        tracing::info!("🧪 TEST MODE ENABLED - Only scanning known servers");
        if args.quick_test {
            tracing::info!("   Quick test: 10 servers only");
        }
    }

    // 1. Load exclude list
    tracing::info!("Loading exclude list from {}...", args.exclude_file);
    let exclude_list = ExcludeList::from_file(&args.exclude_file)
        .unwrap_or_else(|e| {
            tracing::warn!("Could not load {}: {}", args.exclude_file, e);
            tracing::warn!("Using empty exclude list - BE CAREFUL!");
            ExcludeList::from_str("").unwrap()
        });
    tracing::info!("Loaded {} exclude networks", exclude_list.len());

    // 2. Initialize database
    tracing::info!("Initializing database at {}...", args.database);
    let mut opt = ConnectOptions::new(&args.database);
    opt.max_connections(100)
       .acquire_timeout(std::time::Duration::from_secs(30))
       .sqlx_logging(false);
    
    let db = Database::connect(opt).await?;
    
    // Run migrations
    tracing::info!("Running migrations...");
    Migrator::up(&db, None).await?;
    tracing::info!("Migrations applied successfully.");
    
    let db = Arc::new(db);
    
    // Initialize repositories
    let server_repo = Arc::new(ServerRepository::new((*db).clone()));
    let asn_repo = Arc::new(AsnRepository::new((*db).clone()));
    let stats_repo = Arc::new(StatsRepository::new((*db).clone()));
    let api_key_repo = Arc::new(ApiKeyRepository::new((*db).clone()));

    // 3. Initialize ASN fetcher
    tracing::info!("Initializing ASN fetcher...");
    let asn_fetcher = Arc::new(AsnFetcher::new(Arc::clone(&db), Arc::clone(&asn_repo)));
    asn_fetcher.initialize().await?;
    
    // STARTUP SCRUB: Recategorize "unknown" ASNs with latest heuristics.
    // Only runs if no ASN (any category) was updated in the last 7 days.
    // The weekly background refresh (run_background_refresh) handles full
    // re-categorization of ALL ASNs from the iptoasn.com database, so this
    // startup scrub is just a safety net for when the weekly import hasn't run.
    // The timer persists across restarts via the last_updated column in asns.
    {
        use sea_orm::{EntityTrait, QueryFilter, ColumnTrait, QuerySelect};
        let any_recent = crate::models::entities::asns::Entity::find()
            .filter(
                crate::models::entities::asns::Column::LastUpdated
                    .gt(chrono::Utc::now() - chrono::Duration::days(7))
            )
            .limit(1)
            .one(&*db)
            .await
            .unwrap_or_default();

        if any_recent.is_some() {
            tracing::info!("ASN data was updated within 7 days, skipping startup scrub.");
        } else {
            tracing::info!("Running startup ASN recategorization (no updates in > 7 days)...");
            let _ = asn_fetcher.recategorize_all_asns().await;
        }
    }

    tracing::info!(
        "ASN fetcher initialized: {} ASNs, {} ranges",
        asn_fetcher.asn_manager().read().await.asn_count(),
        asn_fetcher.asn_manager().read().await.range_count()
    );

    // GLOBAL DISCOVERY SYNC: Trigger a full import from iptoasn.com if:
    // - Very few ASNs (< 100), OR
    // - Very few ranges (< 100) — indicates old/broken range data, OR
    // - --force-asn-import flag is set
    let range_count = asn_fetcher.asn_manager().read().await.range_count();
    let asn_count = asn_fetcher.asn_manager().read().await.asn_count();
    if asn_count < 100 || range_count < 100 || args.force_asn_import {
        tracing::info!("Running full ASN database import (ASNs: {}, ranges: {}, forced: {})...",
            asn_count, range_count, args.force_asn_import);
        match asn_fetcher.import_full_database().await {
            Ok(()) => tracing::info!("Full ASN import completed successfully."),
            Err(e) => tracing::error!("Full ASN import failed: {}", e),
        }
    }

    // BACKFILL: Fetch ASNs for existing servers that don't have them
    {
        let db_clone = Arc::clone(&db);
        let asn_clone = Arc::clone(&asn_fetcher);
        tokio::spawn(async move {
            let servers_res: Result<Vec<models::entities::servers::Model>, sea_orm::DbErr> = models::entities::servers::Entity::find()
                .filter(models::entities::servers::Column::Asn.is_null())
                .filter(models::entities::servers::Column::Status.ne("ignored"))
                .limit(5000)
                .all(&*db_clone)
                .await;
            if let Ok(servers) = servers_res {
                if !servers.is_empty() {
                    tracing::info!("Backfilling ASN data for {} servers...", servers.len());
                    for server in servers {
                        let _ = asn_clone.fetch_asn_for_ip(&server.ip).await;
                    }
                    tracing::info!("Backfill complete.");
                }
            }
        });
    }

    // 4. Create scanner and scheduler
    let exclude_manager = Arc::new(ExcludeManager::new(&args.exclude_file));
    
    let scanner = Scanner::new(
        Arc::clone(&exclude_manager),
        Arc::clone(&asn_fetcher),
        args.target_rps,
        args.target_concurrency,
        args.target_cold_rps,
    );
    let scheduler = Scheduler::new(
        Arc::clone(&server_repo),
        Arc::clone(&asn_repo),
        args.test_mode || args.quick_test,
        args.test_interval as u32,
    );

    // Load servers based on mode
    if args.test_mode || args.quick_test {
        // Test mode: load known Minecraft servers
        let test_servers: Vec<(String, u16, String, String)> = if args.quick_test {
            test_mode::get_quick_test_servers()
        } else if let Some(ref regions) = args.test_regions {
            test_mode::get_servers_by_region(regions)
        } else {
            test_mode::TestConfig {
                enabled: true,
                max_servers: args.test_max_servers,
                scan_interval: args.test_interval,
                regions: vec![],
            }.get_test_servers()
        };

        tracing::info!("Loading {} test servers...", test_servers.len());
        for (ip, port, name, host) in &test_servers {
            let server_type = if *port == 19132 { "bedrock".to_string() } else { "java".to_string() };
            let mut target = ServerTarget::new(ip.clone(), *port, server_type.clone());
            target.category = AsnCategory::Hosting;
            target.priority = 1; // Hot priority for test servers
            target.hostname = Some(host.clone());
            
            let _ = server_repo.insert_server_if_new(ip, *port as i32, &server_type).await;
            
            scheduler.add_server(target, false).await;
            tracing::debug!("  Added test server: {} ({}:{} as {})", name, ip, port, host);
        }
        tracing::info!("✅ Loaded {} test servers", test_servers.len());
    } else {
        // Production mode: load from database. 
        // Discovery will dynamically fill queues from ASN ranges.
        scheduler.load_from_database().await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load servers from database: {}", e);
        });

        // BACKFILL: Link existing servers to ASNs if data is missing
        match server_repo.link_servers_to_asns().await {
            Ok(count) if count > 0 => tracing::info!("Backfilled ASN data for {} servers", count),
            _ => {}
        }
    }

    let (h, w, c, d) = scheduler.get_queue_sizes().await;
    tracing::info!(
        "Scheduler initialized with queues: Hot={}, Warm={}, Cold={}, Discovery={}",
        h, w, c, d
    );

    let scheduler = Arc::new(scheduler);
    let scanner = Arc::new(scanner);

    // 5. Start background scanner task
    let scanner_handle = {
        let scheduler = Arc::clone(&scheduler);
        let scanner = Arc::clone(&scanner);
        let stats_repo = Arc::clone(&stats_repo);
        let server_repo_clone = Arc::clone(&server_repo);
        
        // Background filler task
        // Background filler task: alternates between discovery and rescan.
        // Every 3rd tick (15s): heavy discovery (warm + cold ASN ranges).
        // Other ticks: rescan known servers from DB whose scan interval elapsed.
        // This gives a 2:1 rescan:discovery ratio, spending more time on known servers.
        let scheduler_filler = Arc::clone(&scheduler);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            let mut tick = 0u64;
            loop {
                interval.tick().await;
                tick += 1;
                if !scheduler_filler.test_mode {
                    // Every 3rd tick: run discovery (adds to discovery_queue)
                    if tick % 3 == 0 {
                        scheduler_filler.fill_warm_queue_if_needed().await;
                        scheduler_filler.fill_cold_queue_if_needed().await;
                    }
                    // Always run refill (re-queues known servers whose interval elapsed)
                    scheduler_filler.try_refill_queues().await;
                }
            }
        });

        tokio::spawn(async move {
            run_scanner_loop(scanner, scheduler, stats_repo, server_repo_clone, args.target_concurrency).await;
        })
    };

    // 6. Start ASN background refresh task
    let asn_refresh_handle = {
        let asn_fetcher = Arc::clone(&asn_fetcher);
        tokio::spawn(async move {
            asn_fetcher.run_background_refresh().await;
        })
    };

    // 6.5. Start Materialized View refresh task
    let mv_refresh_handle = {
        let stats_repo_ref = Arc::clone(&stats_repo);
        tokio::spawn(async move {
            // Give it an initial delay so it doesn't run right at startup
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            
            // Initial refresh
            if let Err(e) = stats_repo_ref.refresh_materialized_views().await {
                tracing::error!("Failed initial refresh of materialized views: {}", e);
            }

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // Every 5 minutes
            loop {
                interval.tick().await;
                if let Err(e) = stats_repo_ref.refresh_materialized_views().await {
                    tracing::error!("Failed to refresh materialized views: {}", e);
                }
            }
        })
    };

    // 7. Create login queue
    let login_queue = Arc::new(LoginQueue::new(Arc::clone(&server_repo)));
    login_queue.start();

    // 8. Start web API
    let api_state = handlers::AppState {
        db: (*db).clone(),
        server_repo: Arc::clone(&server_repo),
        asn_repo: Arc::clone(&asn_repo),
        stats_repo: Arc::clone(&stats_repo),
        api_key_repo: Arc::clone(&api_key_repo),
        scheduler: Arc::clone(&scheduler),
        login_queue: Arc::clone(&login_queue),
        exclude_list: Arc::clone(&exclude_manager),
        api_key: args.api_key.clone(),
        contact_email: args.contact_email.clone(),
        discord_link: args.discord_link.clone(),
    };
    let listen_addr = args.listen_addr.clone();
    let api_handle = tokio::spawn(async move {
        handlers::run_server(api_state, &listen_addr).await.unwrap();
    });

    // Wait for tasks
    tokio::select! {
        _ = scanner_handle => tracing::info!("Scanner stopped"),
        _ = asn_refresh_handle => tracing::info!("ASN refresh stopped"),
        _ = mv_refresh_handle => tracing::info!("MV refresh stopped"),
        _ = api_handle => tracing::info!("API stopped"),
    }

    Ok(())
}

use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

/// Background scanner loop with tiered rate limiting and concurrency.
async fn run_scanner_loop(
    scanner: Arc<Scanner>,
    scheduler: Arc<Scheduler>,
    stats_repo: Arc<StatsRepository>,
    server_repo: Arc<ServerRepository>,
    max_concurrency: u32,
) {
    tracing::info!("Scanner loop started (max concurrency: {})", max_concurrency);

    let hot_count = Arc::new(AtomicU32::new(0));
    let warm_count = Arc::new(AtomicU32::new(0));
    let cold_count = Arc::new(AtomicU32::new(0));
    let active_tasks = Arc::new(AtomicU32::new(0));
    
    // In-memory stats buffer to avoid DB write storm
    let hot_buffer = Arc::new(AtomicU32::new(0));
    let warm_buffer = Arc::new(AtomicU32::new(0));
    let cold_buffer = Arc::new(AtomicU32::new(0));
    let discoveries_buffer = Arc::new(AtomicU32::new(0));

    let mut status_interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    let mut stats_flush_interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    // Result batching system
    let (result_tx, mut result_rx) = mpsc::channel::<crate::network::ScanResult>(max_concurrency as usize * 2);
    let server_repo_clone = Arc::clone(&server_repo);
    
    // Background task for batching DB writes
    tokio::spawn(async move {
        let mut buffer = Vec::with_capacity(100);
        let mut interval = time::interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                res = result_rx.recv() => {
                    if let Some(result) = res {
                        buffer.push(result);
                        if buffer.len() >= 100 {
                            if let Err(e) = server_repo_clone.batch_update_results(buffer.split_off(0)).await {
                                tracing::error!("Failed to batch update results: {}", e);
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        if let Err(e) = server_repo_clone.batch_update_results(buffer.split_off(0)).await {
                            tracing::error!("Failed to batch update results: {}", e);
                        }
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            _ = status_interval.tick() => {
                let (h, w, c, d) = scheduler.get_queue_sizes().await;
                tracing::info!("Queue sizes: Hot={}, Warm={}, Cold={}, Discovery={}", h, w, c, d);
                tracing::info!("Status: Active Tasks={}, Scans today: Hot={}, Warm={}, Cold={}", 
                    active_tasks.load(Ordering::Relaxed),
                    hot_count.load(Ordering::Relaxed), 
                    warm_count.load(Ordering::Relaxed), 
                    cold_count.load(Ordering::Relaxed));
            }
            _ = stats_flush_interval.tick() => {
                // Flush stats to DB
                let h = hot_buffer.swap(0, Ordering::SeqCst);
                let w = warm_buffer.swap(0, Ordering::SeqCst);
                let c = cold_buffer.swap(0, Ordering::SeqCst);
                let d = discoveries_buffer.swap(0, Ordering::SeqCst);
                
                if h > 0 || w > 0 || c > 0 || d > 0 {
                    let _ = stats_repo.increment_batch_stats(h as i32, w as i32, c as i32, d as i32).await;
                }
            }
            server_opt = scheduler.next_server() => {
                if let Some(server) = server_opt {
                    if active_tasks.load(Ordering::SeqCst) >= max_concurrency {
                        // Re-queue immediately if we're at capacity to avoid losing the target
                        scheduler.add_server(server, false).await;
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        continue;
                    }

                    let scanner = Arc::clone(&scanner);
                    let scheduler = Arc::clone(&scheduler);
                    let hot_count = Arc::clone(&hot_count);
                    let warm_count = Arc::clone(&warm_count);
                    let cold_count = Arc::clone(&cold_count);
                    let active_tasks_clone = Arc::clone(&active_tasks);
                    
                    let hot_buffer = Arc::clone(&hot_buffer);
                    let warm_buffer = Arc::clone(&warm_buffer);
                    let cold_buffer = Arc::clone(&cold_buffer);
                    let discoveries_buffer = Arc::clone(&discoveries_buffer);
                    let result_tx = result_tx.clone();

                    active_tasks.fetch_add(1, Ordering::SeqCst);
                    
                    tokio::spawn(async move {
                        let priority = server.priority;
                        
                        // Check if it's a brand new discovery target (never scanned)
                        let is_discovery = server.last_scanned.is_none();

                        let scan_result = scanner
                            .scan_server(&server.ip, server.port, server.hostname.as_deref(), priority, is_discovery, &server.server_type)
                            .await;

                        let was_online = scan_result.as_ref().map(|r| r.online).unwrap_or(false);

                        // Re-queue with updated priority
                        scheduler.requeue_server(server, was_online).await;

                        if let Some(res) = scan_result {
                            let _ = result_tx.send(res).await;
                        }

                        // Track scan counts in memory buffers
                        match priority {
                            1 => {
                                hot_count.fetch_add(1, Ordering::Relaxed);
                                hot_buffer.fetch_add(1, Ordering::Relaxed);
                            }
                            2 => {
                                warm_count.fetch_add(1, Ordering::Relaxed);
                                warm_buffer.fetch_add(1, Ordering::Relaxed);
                            }
                            _ => {
                                cold_count.fetch_add(1, Ordering::Relaxed);
                                cold_buffer.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        if was_online && is_discovery {
                            discoveries_buffer.fetch_add(1, Ordering::Relaxed);
                        }
                        
                        active_tasks_clone.fetch_sub(1, Ordering::SeqCst);
                    });
                } else {
                    // No servers ready — all items have future next_scan_at.
                    // Sleep 2s to avoid tight-loop CPU waste. The refill task
                    // adds new servers every 5s, so we won't miss anything.
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
}
