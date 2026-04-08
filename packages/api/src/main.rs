//! NMCScan API Service
//!
//! Web API and database management service for Minecraft server scanning.
//! This service can run independently of the scanner service.

use migration::Migrator;
use nmcscan_shared::models::entities::{asns, servers};
use nmcscan_shared::repositories::{
    ApiKeyRepository, AsnRepository, ServerRepository, StatsRepository,
};
use nmcscan_shared::services::asn_fetcher::AsnFetcher;
use nmcscan_shared::utils::exclude::{ExcludeList, ExcludeManager};
use sea_orm::{
    ColumnTrait, ConnectOptions, Database, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect,
};
use sea_orm_migration::MigratorTrait;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;

mod handlers;

/// NMCScan API Service arguments
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "NMCScan API Service - Web interface and database management"
)]
struct Args {
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

    /// API listen address
    #[arg(long, env = "LISTEN_ADDR", default_value = "0.0.0.0:3000")]
    listen_addr: String,

    /// Force ASN database re-import from iptoasn.com on startup
    #[arg(long, env = "FORCE_ASN_IMPORT", default_value = "false")]
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

    tracing::info!("🌐 Starting NMCScan API Service...");

    // 1. Load exclude list (with honeypot merging)
    tracing::info!("Loading exclude list from {}...", args.exclude_file);
    let mut exclude_content = std::fs::read_to_string(&args.exclude_file).unwrap_or_else(|e| {
        tracing::warn!("Could not load {}: {}", args.exclude_file, e);
        String::new()
    });

    let honeypot_file = "honeypots.conf";
    if std::path::Path::new(honeypot_file).exists() {
        tracing::info!("Loading honeypot exclusions from {}...", honeypot_file);
        let honeypot_content = std::fs::read_to_string(honeypot_file).unwrap_or_else(|e| {
            tracing::warn!("Could not load {}: {}", honeypot_file, e);
            String::new()
        });

        if !honeypot_content.is_empty() {
            exclude_content.push_str("\n# Honeypot exclusions from honeypots.conf\n");
            exclude_content.push_str(&honeypot_content);
            tracing::info!("Merged honeypot exclusions into main exclude list");
        }
    }

    let exclude_list = ExcludeList::from_str(&exclude_content).unwrap_or_else(|e| {
        tracing::warn!("Failed to parse exclude list: {}", e);
        ExcludeList::from_str("").unwrap()
    });
    tracing::info!(
        "Loaded {} exclude networks (including honeypots)",
        exclude_list.len()
    );

    // 2. Initialize database and run migrations
    tracing::info!("Initializing database at {}...", args.database);
    let mut opt = ConnectOptions::new(&args.database);
    opt.max_connections(200)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .sqlx_logging(false);

    let db = Database::connect(opt).await?;

    tracing::info!("Running migrations...");
    Migrator::up(&db, None).await?;
    tracing::info!("Migrations applied successfully.");

    let db = Arc::new(db);

    // 3. Initialize repositories
    let server_repo = Arc::new(ServerRepository::new((*db).clone()));
    let asn_repo = Arc::new(AsnRepository::new((*db).clone()));
    let stats_repo = Arc::new(StatsRepository::new((*db).clone()));
    let api_key_repo = Arc::new(ApiKeyRepository::new((*db).clone()));
    let minecraft_account_repo =
        Arc::new(nmcscan_shared::repositories::MinecraftAccountRepository::new((*db).clone()));

    // 4. Initialize ASN fetcher and import/update ASN data
    tracing::info!("Initializing ASN fetcher...");
    let asn_fetcher = Arc::new(AsnFetcher::new(Arc::clone(&db), Arc::clone(&asn_repo)));

    // Full import if forced or if data is missing
    let range_count = asn_fetcher.asn_manager().read().await.range_count();
    let asn_count = asn_fetcher.asn_manager().read().await.asn_count();
    if asn_count < 100 || range_count < 100 || args.force_asn_import {
        tracing::info!(
            "Running full ASN database import (ASNs: {}, ranges: {})...",
            asn_count,
            range_count
        );
        match asn_fetcher.import_full_database().await {
            Ok(()) => tracing::info!("Full ASN import completed successfully."),
            Err(e) => tracing::error!("Full ASN import failed: {}", e),
        }
    }

    // Run startup recategorization if needed
    {
        let any_recent = asns::Entity::find()
            .filter(asns::Column::LastUpdated.gt(chrono::Utc::now() - chrono::Duration::days(7)))
            .limit(1)
            .one(&*db)
            .await
            .unwrap_or_default();

        let unknown_count = asns::Entity::find()
            .filter(asns::Column::Category.eq("unknown"))
            .count(&*db)
            .await
            .unwrap_or(0);

        let total_count = asns::Entity::find().count(&*db).await.unwrap_or(1);

        let unknown_percentage = if total_count > 0 {
            (unknown_count as f64 / total_count as f64) * 100.0
        } else {
            0.0
        };

        if any_recent.is_some() && unknown_percentage < 50.0 {
            tracing::info!(
                "ASN data was updated within 7 days ({}% unknown), skipping startup scrub.",
                unknown_percentage
            );
        } else {
            tracing::info!("Running startup ASN recategorization...");
            let ipverse_map = asn_fetcher.fetch_ipverse_map().await;
            match asn_fetcher.recategorize_all_asns(&ipverse_map).await {
                Ok(updated) => {
                    tracing::info!(
                        "Startup recategorization complete: {} ASNs reclassified",
                        updated
                    );
                }
                Err(e) => {
                    tracing::error!("Startup recategorization failed: {}", e);
                }
            }
        }
    }

    // 5. Backfill ASN data for existing servers
    {
        let db_clone = Arc::clone(&db);
        let servers_res: Result<Vec<servers::Model>, sea_orm::DbErr> = servers::Entity::find()
            .filter(servers::Column::Asn.is_null())
            .filter(servers::Column::Status.ne("ignored"))
            .limit(5000)
            .all(&*db_clone)
            .await;
        if let Ok(srvs) = servers_res {
            if !srvs.is_empty() {
                tracing::info!("Backfilling ASN data for {} servers...", srvs.len());
                // Note: Full backfill would require the ASN fetcher to look up each IP
                // This is a lighter version for the API service
                tracing::info!(
                    "Backfill skipped in API-only mode (scanner will handle on startup)"
                );
            }
        }
    }

    let exclude_manager = Arc::new(ExcludeManager::new(&args.exclude_file));

    // 6. Start login queue
    // 7. Start Materialized View refresh task (every 5 minutes)
    let mv_refresh_handle = {
        let stats_repo_ref = Arc::clone(&stats_repo);
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            if let Err(e) = stats_repo_ref.refresh_materialized_views().await {
                tracing::error!("Failed initial refresh of materialized views: {}", e);
            }

            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                if let Err(e) = stats_repo_ref.refresh_materialized_views().await {
                    tracing::error!("Failed to refresh materialized views: {}", e);
                }
            }
        })
    };

    // 8. Start background ASN refresh task (weekly recategorization)
    let asn_refresh_handle = {
        let asn_fetcher = Arc::clone(&asn_fetcher);
        tokio::spawn(async move {
            asn_fetcher.run_background_refresh().await;
        })
    };

    // 9. Start web API
    // Note: Scheduler is not available in API-only mode
    // The handlers will check for None and return appropriate responses
    let api_state = handlers::AppState {
        db: (*db).clone(),
        server_repo: Arc::clone(&server_repo),
        asn_repo: Arc::clone(&asn_repo),
        stats_repo: Arc::clone(&stats_repo),
        api_key_repo: Arc::clone(&api_key_repo),
        minecraft_account_repo: Arc::clone(&minecraft_account_repo),
        scheduler: None, // Not available in API-only mode
        exclude_list: Arc::clone(&exclude_manager),
        api_key: args.api_key.clone(),
        contact_email: args.contact_email.clone(),
        discord_link: args.discord_link.clone(),
    };

    let listen_addr = args.listen_addr.clone();
    let listen_addr_log = listen_addr.clone();
    let api_handle = tokio::spawn(async move {
        handlers::run_server(api_state, &listen_addr).await.unwrap();
    });

    tracing::info!("✅ API Service started on {}", listen_addr_log);
    tracing::info!("   Database: connected");
    tracing::info!(
        "   ASN Manager: initialized ({} ASNs, {} ranges)",
        asn_count,
        range_count
    );
    tracing::info!("   Scanner: NOT running (separate nmcscan-scanner service)");
    tracing::info!("   Login queue: NOT running (part of scanner service)");

    // Wait for tasks
    tokio::select! {
        _ = asn_refresh_handle => tracing::info!("ASN refresh stopped"),
        _ = mv_refresh_handle => tracing::info!("MV refresh stopped"),
        _ = api_handle => tracing::info!("API stopped"),
    }

    Ok(())
}
