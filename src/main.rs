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

mod api;
mod asn;
mod asn_fetcher;
mod db;
mod exclude;
mod scheduler;
mod scanner;
mod slp;
mod test_mode;

use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    tracing::info!("Loading exclude list...");
    let exclude_list = exclude::ExcludeList::from_file("exclude.conf")
        .unwrap_or_else(|e| {
            tracing::warn!("Could not load exclude.conf: {}", e);
            tracing::warn!("Using empty exclude list - BE CAREFUL!");
            exclude::ExcludeList::from_str("").unwrap()
        });
    tracing::info!("Loaded {} exclude networks", exclude_list.len());

    // 2. Initialize database
    tracing::info!("Initializing database...");
    let db = db::Database::new("nmcscan.db").await?;
    let db = Arc::new(db);

    // 3. Initialize ASN fetcher
    tracing::info!("Initializing ASN fetcher...");
    let asn_fetcher = Arc::new(asn_fetcher::AsnFetcher::new(Arc::clone(&db)));
    asn_fetcher.initialize().await?;
    tracing::info!(
        "ASN fetcher initialized: {} ASNs, {} ranges",
        asn_fetcher.asn_manager().read().await.asn_count(),
        asn_fetcher.asn_manager().read().await.range_count()
    );

    // 4. Create scanner and scheduler
    let scanner = scanner::Scanner::new(
        exclude::ExcludeList::from_file("exclude.conf").unwrap_or_else(|_| {
            exclude::ExcludeList::from_str("").unwrap()
        }),
        Arc::clone(&db),
    );
    let mut scheduler = scheduler::Scheduler::new(Arc::clone(&db));

    // Load servers based on mode
    if args.test_mode || args.quick_test {
        // Test mode: load known Minecraft servers
        scheduler.test_mode = true;
        scheduler.test_interval = args.test_interval;
        
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
            let mut target = scheduler::ServerTarget::new(ip.clone(), *port);
            target.category = asn::AsnCategory::Hosting;
            target.priority = 1; // Hot priority for test servers
            target.hostname = Some(host.clone());
            
            let _ = db.insert_server_if_new(ip, *port as i32).await;
            
            scheduler.add_server(target).await;
            tracing::debug!("  Added test server: {} ({}:{} as {})", name, ip, port, host);
        }
        tracing::info!("✅ Loaded {} test servers", test_servers.len());
    } else {
        // Production mode: load from database and ASN ranges
        scheduler.load_from_database().await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load servers from database: {}", e);
        });

        // Pre-populate Warm queue with hosting ASN ranges
        for (cidr, asn) in scheduler::HOSTING_ASN_RANGES {
            scheduler
                .add_asn_range_servers(cidr, asn)
                .await
                .unwrap_or_else(|e| {
                    tracing::debug!("Failed to add servers from {} ({}): {}", cidr, asn, e);
                });
        }
    }

    tracing::info!(
        "Scheduler initialized with queues: Hot={}, Warm={}, Cold={}",
        scheduler.get_queue_sizes().await.0,
        scheduler.get_queue_sizes().await.1,
        scheduler.get_queue_sizes().await.2
    );

    let scheduler = Arc::new(scheduler);

    // 5. Start background scanner task
    let scanner_handle = {
        let scheduler = Arc::clone(&scheduler);
        let scanner = Arc::new(scanner);
        tokio::spawn(async move {
            run_scanner_loop(scanner, scheduler).await;
        })
    };

    // 6. Start ASN background refresh task
    let asn_refresh_handle = {
        let asn_fetcher = Arc::clone(&asn_fetcher);
        tokio::spawn(async move {
            asn_fetcher.run_background_refresh().await;
        })
    };

    // 7. Start web API
    let api_state = api::AppState {
        db: Arc::clone(&db),
        scheduler: Arc::clone(&scheduler),
        api_key: args.api_key.clone(),
    };
    let api_handle = tokio::spawn(async move {
        api::run_server(api_state, "0.0.0.0:3000").await.unwrap();
    });

    // Wait for tasks
    tokio::select! {
        _ = scanner_handle => tracing::info!("Scanner stopped"),
        _ = asn_refresh_handle => tracing::info!("ASN refresh stopped"),
        _ = api_handle => tracing::info!("API stopped"),
    }

    Ok(())
}

use std::sync::atomic::{AtomicU32, Ordering};

/// Background scanner loop with tiered rate limiting and concurrency.
async fn run_scanner_loop(
    scanner: Arc<scanner::Scanner>,
    scheduler: Arc<scheduler::Scheduler>,
) {
    tracing::info!("Scanner loop started");

    let hot_count = Arc::new(AtomicU32::new(0));
    let warm_count = Arc::new(AtomicU32::new(0));
    let cold_count = Arc::new(AtomicU32::new(0));
    
    // Concurrency limit for task spawning to avoid memory pressure
    let spawn_semaphore = Arc::new(tokio::sync::Semaphore::new(1000));

    let mut status_interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    let mut fill_interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = status_interval.tick() => {
                let (h, w, c) = scheduler.get_queue_sizes().await;
                tracing::info!("Queue sizes: Hot={}, Warm={}, Cold={}", h, w, c);
                tracing::info!("Scans today: Hot={}, Warm={}, Cold={}", 
                    hot_count.load(Ordering::Relaxed), 
                    warm_count.load(Ordering::Relaxed), 
                    cold_count.load(Ordering::Relaxed));
            }
            _ = fill_interval.tick() => {
                if !scheduler.test_mode {
                    scheduler.fill_warm_queue_if_needed().await;
                    scheduler.fill_cold_queue_if_needed().await;
                }
            }
            server_opt = scheduler.next_server() => {
                if let Some(server) = server_opt {
                    let scanner = Arc::clone(&scanner);
                    let scheduler = Arc::clone(&scheduler);
                    let hot_count = Arc::clone(&hot_count);
                    let warm_count = Arc::clone(&warm_count);
                    let cold_count = Arc::clone(&cold_count);
                    
                    // Wait for a spawn slot
                    let permit = match spawn_semaphore.clone().acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => break, // Should not happen
                    };

                    tokio::spawn(async move {
                        let _permit = permit;
                        let category = server.category.clone();

                        let was_online = scanner
                            .scan_server(&server.ip, server.port, server.hostname.as_deref())
                            .await;

                        // Re-queue with updated priority
                        scheduler.requeue_server(server, was_online).await;

                        // Track scan counts per tier
                        match category {
                            asn::AsnCategory::Hosting => {
                                hot_count.fetch_add(1, Ordering::Relaxed);
                            }
                            asn::AsnCategory::Unknown => {
                                warm_count.fetch_add(1, Ordering::Relaxed);
                            }
                            asn::AsnCategory::Residential => {
                                cold_count.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    });
                } else {
                    // No servers ready, sleep a bit to avoid CPU spin
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }
        }
    }
}
