use crate::scanner::Scanner;
use nmcscan_shared::repositories::{ServerRepository, StatsRepository};
use nmcscan_shared::services::scheduler::{ScanPassResult, Scheduler};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;
use tokio::time::Duration;

/// Background scanner loop with 3-pass pipeline.
/// 
/// Pass flow:
/// - Pass 0 (initial): TCP Connect scan (Pass 1)
/// - Pass 1: SLP/RakNet scan (Pass 2)
/// - Pass 3: Fully scanned, scheduled for rescan
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
    let tcp_pass_buffer = Arc::new(AtomicU32::new(0));
    let slp_pass_buffer = Arc::new(AtomicU32::new(0));

    let mut status_interval = tokio::time::interval(Duration::from_secs(60));
    let mut stats_flush_interval = tokio::time::interval(Duration::from_secs(10));

    // Result batching channel for SLP results (servers that pass Pass 2)
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
                let tcp = tcp_pass_buffer.swap(0, Ordering::SeqCst);
                let slp = slp_pass_buffer.swap(0, Ordering::SeqCst);
                if s > 0 || d > 0 || tcp > 0 || slp > 0 {
                    let _ = stats_repo.increment_batch_stats(s as i32, 0, d as i32, 0).await;
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
                    let tcp_pass_buffer = Arc::clone(&tcp_pass_buffer);
                    let slp_pass_buffer = Arc::clone(&slp_pass_buffer);
                    let result_tx = result_tx.clone();

                    active_tasks.fetch_add(1, Ordering::SeqCst);

                    tokio::spawn(async move {
                        let is_discovery = server.is_discovery;
                        let current_pass = server.pass;

                        // Determine which pass to run
                        let pass_result = if current_pass == 0 {
                            // Pass 1: TCP Connect scan
                            let tcp_ok = scanner
                                .scan_tcp(&server.ip, server.port)
                                .await;

                            tcp_pass_buffer.fetch_add(1, Ordering::Relaxed);

                            if tcp_ok {
                                ScanPassResult::TcpPassed
                            } else {
                                ScanPassResult::TcpFailed
                            }
                        } else if current_pass == 1 {
                            // Pass 2: SLP/RakNet scan
                            let scan_result = scanner
                                .scan_slp(
                                    &server.ip,
                                    server.port,
                                    server.hostname.as_deref(),
                                    server.priority,
                                    is_discovery,
                                    &server.server_type,
                                )
                                .await;

                            let slp_ok = scan_result.as_ref().map(|r| r.online).unwrap_or(false);

                            // Send to DB if SLP passed
                            if slp_ok {
                                slp_pass_buffer.fetch_add(1, Ordering::Relaxed);
                                if let Some(res) = scan_result {
                                    let _ = result_tx.send(res).await;
                                }
                            }

                            if slp_ok {
                                ScanPassResult::SlpPassed
                            } else {
                                ScanPassResult::SlpFailed
                            }
                        } else {
                            // Pass 3+: Already fully scanned, treat as SLP passed for rescan
                            // This shouldn't normally happen, but handle gracefully
                            ScanPassResult::SlpPassed
                        };

                        // Requeue based on pass result
                        scheduler.requeue_server(server, pass_result).await;

                        scan_buffer.fetch_add(1, Ordering::Relaxed);
                        if pass_result == ScanPassResult::SlpPassed && is_discovery {
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