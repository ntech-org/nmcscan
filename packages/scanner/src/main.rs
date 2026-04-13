//! NMCScan Scanner Service
//!
//! High-performance Minecraft server scanning service.
//! This service can run independently of the API service.

mod login_queue;
mod scanner;
mod scanner_http;
mod scanner_loop;

use crate::scanner::Scanner;
use crate::scanner_http::{run_http_server, ScannerState};
use nmcscan_shared::models::asn::AsnCategory;
use nmcscan_shared::repositories::{AsnRepository, ExclusionRepository, ServerRepository, StatsRepository};
use nmcscan_shared::services::asn_fetcher::AsnFetcher;
use nmcscan_shared::services::scheduler::{Scheduler, ServerTarget};
use nmcscan_shared::utils::exclude::{ExcludeList, ExcludeManager};
use nmcscan_shared::utils::test_mode;
use sea_orm::{ConnectOptions, Database};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;

/// NMCScan Scanner Service arguments
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "NMCScan Scanner Service - High-performance Minecraft server scanning"
)]
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

    /// PostgreSQL database URL
    #[arg(
        short,
        long,
        env = "DATABASE_URL",
        default_value = "postgres://nmcscan:nmcscan_secret@localhost:5432/nmcscan"
    )]
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

    /// Force ASN database re-import from iptoasn.com on startup
    #[arg(long, env = "FORCE_ASN_IMPORT", default_value = "false")]
    force_asn_import: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or(args.log_level),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("🔍 Starting NMCScan Scanner Service...");

    if args.test_mode || args.quick_test {
        tracing::info!("🧪 TEST MODE ENABLED");
    }

    // 1. Load exclude list
    tracing::info!("Loading exclude list from {}...", args.exclude_file);
    let mut exclude_content = std::fs::read_to_string(&args.exclude_file).unwrap_or_else(|e| {
        tracing::warn!("Could not load {}: {}", args.exclude_file, e);
        String::new()
    });

    let honeypot_file = "honeypots.conf";
    if std::path::Path::new(honeypot_file).exists() {
        let honeypot_content = std::fs::read_to_string(honeypot_file).unwrap_or_else(|e| {
            tracing::warn!("Could not load {}: {}", honeypot_file, e);
            String::new()
        });
        if !honeypot_content.is_empty() {
            exclude_content.push_str("\n# Honeypot exclusions\n");
            exclude_content.push_str(&honeypot_content);
        }
    }

    let exclude_list = ExcludeList::from_str(&exclude_content).unwrap_or_else(|e| {
        tracing::warn!("Failed to parse exclude list: {}", e);
        ExcludeList::from_str("").unwrap()
    });
    tracing::info!("Loaded {} exclude networks", exclude_list.len());

    // 2. Initialize database
    let mut opt = ConnectOptions::new(&args.database);
    opt.max_connections(200)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .sqlx_logging(false);

    let db = Database::connect(opt).await?;
    // Migrations are handled by the API service. The scanner depends on the API
    // healthcheck, so migrations are guaranteed to be applied before startup.
    tracing::info!("Skipping migrations (handled by API service).");
    let db = Arc::new(db);

    // 3. Initialize repositories
    let server_repo = Arc::new(ServerRepository::new((*db).clone()));
    let asn_repo = Arc::new(AsnRepository::new((*db).clone()));
    let stats_repo = Arc::new(StatsRepository::new((*db).clone()));
    let exclusion_repo = Arc::new(ExclusionRepository::new((*db).clone()));

    // 4. Initialize ASN fetcher
    let asn_fetcher = Arc::new(AsnFetcher::new(Arc::clone(&db), Arc::clone(&asn_repo)));
    asn_fetcher.initialize().await?;

    // Import ASN database if needed
    let range_count = asn_fetcher.asn_manager().read().await.range_count();
    let asn_count = asn_fetcher.asn_manager().read().await.asn_count();
    if asn_count < 100 || range_count < 100 || args.force_asn_import {
        let clean_slate = args.force_asn_import || asn_count > 0;
        tracing::info!("Running full ASN database import (clean: {})...", clean_slate);
        match asn_fetcher.import_full_database(clean_slate).await {
            Ok(()) => tracing::info!("Full ASN import completed."),
            Err(e) => tracing::error!("Full ASN import failed: {}", e),
        }
    }

    // 5. Create scanner and scheduler
    let exclude_manager = Arc::new(ExcludeManager::new(&args.exclude_file));

    // Merge exclusions from database into in-memory list (DB is source of truth for runtime changes)
    if let Ok(db_exclusions) = exclusion_repo.get_all_networks().await {
        let db_count = db_exclusions.len();
        for network in db_exclusions {
            exclude_manager.insert_network(&network).await;
        }
        if db_count > 0 {
            tracing::info!("Merged {} exclusions from database", db_count);
        }
    }

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
        args.target_rps,
    );

    // 6. Load servers
    if args.test_mode || args.quick_test {
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
            }
            .get_test_servers()
        };

        tracing::info!("Loading {} test servers...", test_servers.len());
        for (ip, port, _name, host) in &test_servers {
            let server_type: String = if *port == 19132 {
                "bedrock".to_string()
            } else {
                "java".to_string()
            };
            let mut target = ServerTarget::new(ip.clone(), *port, server_type.clone());
            target.category = AsnCategory::Hosting;
            target.priority = 1;
            target.hostname = Some(host.clone());

            let port_i16: i16 = (*port).try_into().unwrap_or(25565);
            let _ = server_repo
                .insert_server_if_new(ip, port_i16, &server_type)
                .await;
            scheduler.add_server(target, false).await;
        }
    }
    // Non-test mode: queues start empty and fill naturally via background tasks
    // (try_refill_queues, fill_warm_queue_if_needed, fill_cold_queue_if_needed)

    // Load known server IPs into the discovery skip-list.
    // This prevents discovery from re-scanning IPs that are already known servers.
    if let Ok(count) = scheduler.load_known_servers().await {
        tracing::info!("Discovery skip-list: {} known server IPs loaded from DB", count);
    }

    let ((h_ready, h_total), (w_ready, w_total), (c_ready, c_total), d) = scheduler.get_queue_readiness().await;
    tracing::info!(
        "Scheduler queues: Hot={}/{} ready/total, Warm={}/{} ready/total, Cold={}/{} ready/total, Discovery={}",
        h_ready, h_total, w_ready, w_total, c_ready, c_total, d
    );

    let scheduler = Arc::new(scheduler);
    let scanner = Arc::new(scanner);

    // 7. Start login queue (performs actual login attempts to servers)
    let login_queue = Arc::new(login_queue::LoginQueue::new(Arc::clone(&server_repo)));
    login_queue.start();
    tracing::info!("Login queue started");

    // 8. Start HTTP server for API communication (status + control)
    let http_state = ScannerState {
        scheduler: Arc::clone(&scheduler),
        login_queue: Arc::clone(&login_queue),
    };
    let http_handle = tokio::spawn(async move {
        if let Err(e) = run_http_server(http_state, "0.0.0.0:3001").await {
            tracing::error!("Scanner HTTP server error: {}", e);
        }
    });

    // 9. Start background tasks
    let scheduler_filler = Arc::clone(&scheduler);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        let mut tick = 0u64;
        loop {
            interval.tick().await;
            tick += 1;
            if !scheduler_filler.test_mode {
                if tick % 3 == 0 {
                    scheduler_filler.fill_warm_queue_if_needed().await;
                    scheduler_filler.fill_cold_queue_if_needed().await;
                }
                scheduler_filler.try_refill_queues().await;
            }
        }
    });

    let scanner_handle = {
        let scheduler = Arc::clone(&scheduler);
        let scanner = Arc::clone(&scanner);
        let stats_repo = Arc::clone(&stats_repo);
        let server_repo = Arc::clone(&server_repo);
        tokio::spawn(async move {
            scanner_loop::run_scanner_loop(
                scanner,
                scheduler,
                stats_repo,
                server_repo,
                args.target_concurrency,
            )
            .await;
        })
    };

    let asn_refresh_handle = {
        let asn_fetcher = Arc::clone(&asn_fetcher);
        tokio::spawn(async move {
            asn_fetcher.run_background_refresh().await;
        })
    };

    tracing::info!("✅ Scanner Service started");
    tracing::info!("   ASN Manager: {} ASNs, {} ranges", asn_count, range_count);
    tracing::info!("   Scanner HTTP API: listening on 0.0.0.0:3001");

    // Wait for tasks
    tokio::select! {
        _ = scanner_handle => tracing::info!("Scanner stopped"),
        _ = asn_refresh_handle => tracing::info!("ASN refresh stopped"),
        _ = http_handle => tracing::info!("HTTP server stopped"),
    }

    Ok(())
}
