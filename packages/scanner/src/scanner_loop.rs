use crate::scanner::Scanner;
use nmcscan_shared::repositories::{ServerRepository, StatsRepository};
use nmcscan_shared::services::scheduler::Scheduler;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio::time::Duration;

/// Background scanner loop with concurrency control.
/// Simplified: single unified queue, no complex tier logic.
pub async fn run_scanner_loop(
    scanner: Arc<Scanner>,
    scheduler: Arc<Scheduler>,
    stats_repo: Arc<StatsRepository>,
    server_repo: Arc<ServerRepository>,
    max_concurrency: u32,
) {
    tracing::info!("Scanner loop started (max concurrency: {})", max_concurrency);

    let active_tasks = Arc::new(AtomicU32::new(0));
    let total_scans = Arc::new(AtomicU32::new(0));

    // In-memory stats buffer
    let scan_buffer = Arc::new(AtomicU32::new(0));
    let discovery_buffer = Arc::new(AtomicU32::new(0));

    let mut status_interval = tokio::time::interval(Duration::from_secs(60));
    let mut stats_flush_interval = tokio::time::interval(Duration::from_secs(10));

    // Result batching channel
    let (result_tx, mut result_rx) =
        mpsc::channel::<nmcscan_shared::network::ScanResult>(max_concurrency as usize * 2);
    let server_repo_clone = Arc::clone(&server_repo);

    // Background task for batching DB writes
    tokio::spawn(async move {
        let mut buffer = Vec::with_capacity(100);
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                res = result_rx.recv() => {
                    if let Some(result) = res {
                        buffer.push(result);
                        if buffer.len() >= 100 {
                            flush_results(&server_repo_clone, &mut buffer).await;
                        }
                    } else {
                        break;
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        flush_results(&server_repo_clone, &mut buffer).await;
                    }
                }
            }
        }
    });

    loop {
        tokio::select! {
            _ = status_interval.tick() => {
                let queue_stats = scheduler.get_queue_stats().await;
                tracing::info!(
                    "Queue: {}/{} ready/total, Discovery dedup={}, Active={}, Scans today={}",
                    queue_stats.ready,
                    queue_stats.total,
                    queue_stats.discovery,
                    active_tasks.load(Ordering::Relaxed),
                    total_scans.load(Ordering::Relaxed)
                );
            }
            _ = stats_flush_interval.tick() => {
                let s = scan_buffer.swap(0, Ordering::SeqCst);
                let d = discovery_buffer.swap(0, Ordering::SeqCst);
                if s > 0 || d > 0 {
                    let _ = stats_repo.increment_batch_stats(s as i32, 0, 0, d as i32).await;
                }
            }
            server_opt = scheduler.next_server() => {
                if let Some(server) = server_opt {
                    if active_tasks.load(Ordering::SeqCst) >= max_concurrency {
                        scheduler.add_server(server).await;
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }

                    let scanner = Arc::clone(&scanner);
                    let scheduler = Arc::clone(&scheduler);
                    let active_tasks_clone = Arc::clone(&active_tasks);
                    let total_scans = Arc::clone(&total_scans);
                    let scan_buffer = Arc::clone(&scan_buffer);
                    let discovery_buffer = Arc::clone(&discovery_buffer);
                    let result_tx = result_tx.clone();

                    active_tasks.fetch_add(1, Ordering::SeqCst);

                    tokio::spawn(async move {
                        let is_discovery = server.last_scanned.is_none();

                        let scan_result = scanner
                            .scan_server(
                                &server.ip,
                                server.port,
                                server.hostname.as_deref(),
                                server.priority,
                                is_discovery,
                                &server.server_type,
                            )
                            .await;

                        let was_online = scan_result.as_ref().map(|r| r.online).unwrap_or(false);

                        scheduler.requeue_server(server, was_online).await;

                        if let Some(res) = scan_result {
                            let _ = result_tx.send(res).await;
                        }

                        scan_buffer.fetch_add(1, Ordering::Relaxed);
                        if was_online && is_discovery {
                            discovery_buffer.fetch_add(1, Ordering::Relaxed);
                        }
                        total_scans.fetch_add(1, Ordering::Relaxed);

                        active_tasks_clone.fetch_sub(1, Ordering::SeqCst);
                    });
                } else {
                    // Nothing ready — sleep briefly. The discovery fill task
                    // adds new targets every 15s, so we won't miss anything.
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }
}

async fn flush_results(
    repo: &ServerRepository,
    buffer: &mut Vec<nmcscan_shared::network::ScanResult>,
) {
    if let Err(e) = repo.batch_update_results(buffer.split_off(0)).await {
        tracing::error!("Failed to batch update results: {}", e);
    }
}
