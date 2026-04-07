use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use nmcscan_shared::network::ScanResult;
use crate::scanner::Scanner;
use nmcscan_shared::repositories::{ServerRepository, StatsRepository};
use nmcscan_shared::services::scheduler::Scheduler;

// Background scanner loop with tiered rate limiting and concurrency.
pub async fn run_scanner_loop(
    scanner: Arc<Scanner>,
    scheduler: Arc<Scheduler>,
    stats_repo: Arc<StatsRepository>,
    server_repo: Arc<ServerRepository>,
    max_concurrency: u32,
) {
    tracing::info!(
        "Scanner loop started (max concurrency: {})",
        max_concurrency
    );

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
    let (result_tx, mut result_rx) =
        mpsc::channel::<nmcscan_shared::network::ScanResult>(max_concurrency as usize * 2);
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
                            .scan_server(server.ip.to_string().as_str(), server.port, server.hostname.as_deref(), priority, is_discovery, &server.server_type)
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
