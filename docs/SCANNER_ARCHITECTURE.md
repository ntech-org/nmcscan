# Scanner Architecture

## Overview

The scanner service (`nmcscan-scanner`) is an autonomous worker that continuously probes the IPv4 internet for Minecraft servers. It operates independently of the API service, communicating only through PostgreSQL. The scanner runs in a single process with multiple concurrent background tasks managed by Tokio.

---

## Process Startup (`packages/scanner/src/main.rs`)

1. **Exclude list** — `exclude.conf` + `honeypots.conf` are loaded into `ExcludeManager`. Any IP matching these ranges is silently skipped before any network connection.
2. **Database** — Connects to PostgreSQL (pool size 200), runs migrations.
3. **ASN subsystem** — Initializes the in-memory `AsnManager` from local MaxMind GeoLite2 databases (`.mmdb` files). If the database is missing or stale, downloads it from a public mirror. Optionally imports the full `iptoasn.com` dataset on first run.
4. **Scanner** — Created with two independent token-bucket rate limiters:
   - **Hot/Warm limiter**: full `TARGET_RPS` (default 100)
   - **Cold limiter**: `TARGET_COLD_RPS` (default 10, i.e. 10% of RPS)
5. **Scheduler** — Priority-based queue manager with four internal queues (hot, warm, cold, discovery). All queues start empty in production mode.
6. **Test mode** — If `TEST_MODE=true`, a fixed list of servers is loaded directly into the hot queue, bypassing discovery.
7. **Login queue** — Background task that attempts actual Minecraft protocol logins on online servers (see §5).
8. **Background tasks** spawned:
   - Scheduler refill (every 5s)
   - Scanner main loop
   - ASN background refresh (weekly)

---

## Scheduler (`packages/shared/src/services/scheduler.rs`)

The scheduler maintains four in-memory `VecDeque<ServerTarget>` wrapped in `Arc<Mutex<>>`. Each queue has a hard cap of **100,000 items**.

### Queue Architecture

| Queue | Priority | Purpose | Scan Interval |
|-------|----------|---------|---------------|
| Hot | 1 | Known active servers (previously online) | 2 hours |
| Warm | 2 | New hosting ASN IPs + servers missed 24h+ | 24 hours |
| Cold | 3 | New residential ASN IPs + dead servers | 7 days |
| Discovery | N/A | Feeder for Warm and Cold — unscanned IPs | Immediate |

The discovery queue is **not** a scanning tier. It is a staging area. When a discovery target is scanned for the first time, it graduates to the appropriate main queue based on the scan result:
- **Online** → Hot (priority 1)
- **Offline + hosting ASN** → stays in discovery for warm/cold refill cycles
- **Offline + residential ASN** → stays in discovery for cold refill cycles

### Server Selection Algorithm (`next_server`)

Called by the scanner loop to fetch the next target. Uses a **most-overdue-first** strategy:

1. **Discovery queue** — Pops from the front (O(1)). If a target exists, it is returned immediately. Discovery targets have `next_scan_at = None`, meaning they are always ready.
2. **Earliest-ready search** — For each of the three main queues (hot, warm, cold), scans up to 5,000 items to find the one with the earliest `next_scan_at <= now`. This is a linear search, not a heap.
3. **Tier selection** — Among the three "earliest-ready" candidates (one per queue), picks the one with the most overdue `next_scan_at`. This ensures the most-starved tier gets served first.
4. **Deep scan fallback** — If nothing was found in the first 5,000 items of any queue, scans the entire queue length for any ready item.
5. **Sleep** — If truly nothing is ready, returns `None`, causing the scanner loop to sleep for 2 seconds.

### Queue Refill (`try_refill_queues`)

Runs every 5 seconds from the background task. For each tier:

```
Hot:    refill when queue < 2,500 items, fetch up to 5,000 from DB (stale > 2h)
Warm:   refill when queue < 1,250 items, fetch up to 3,000 from DB (stale > 24h)
Cold:   refill when queue < 1,250 items, fetch up to 2,000 from DB (stale > 168h)
```

The DB query (`get_servers_for_refill`) filters:
```sql
WHERE priority = X
  AND status != 'ignored'
  AND (last_seen IS NULL OR last_seen < now - interval 'X hours')
ORDER BY last_seen ASC
LIMIT N
```

This makes the refill **stateless** — no `next_scan_at` is persisted. The staleness of `last_seen` implicitly determines which servers need rescanning. On crash recovery, queues start empty and naturally repopulate as stale servers are detected by the refill queries.

### Discovery Queue Filling (`fill_warm_queue_if_needed` / `fill_cold_queue_if_needed`)

Runs every 15 seconds (every 3rd tick of the 5s background task).

**Warm queue** (hosting ASNs):
1. Fetches up to 500 hosting-category `asn_ranges` from DB, ordered by `last_scanned_at ASC`, randomized.
2. For each range, generates 200 IPs using deterministic hash-based shuffle at the current `scan_offset`.
3. Both Java (25565) and Bedrock (19132) targets are created per IP.
4. Targets are shuffled and pushed to the discovery queue.
5. Batch-updates `scan_offset` and `scan_epoch` in DB via a single `UPDATE ... FROM unnest()` query.

**Cold queue** (residential ASNs):
1. First tries to recycle 1,000 dead/ignored servers from the `servers` table into the cold queue.
2. Then fetches up to 500 residential-category `asn_ranges` and generates 200 IPs each (same process as warm).

**Range exhaustion & epoch cycling**:
When a range's `scan_offset` reaches the total IP count (`network.size()`):
- The range enters a **cooldown period**: 12 hours for hosting, 56 hours for residential.
- After cooldown, `scan_offset` resets to 0 and `scan_epoch` increments by 1.
- The new epoch produces a **different shuffled permutation** of the same IP range (seeded by `hash(cidr + epoch)`), ensuring no IP is scanned in the same order twice.

### Requeue Logic (`requeue_server`)

After each scan, the server is re-queued with updated state:

```
If online:
    priority = 1 (hot)
    consecutive_failures = 0
    delay = 2h

If offline:
    consecutive_failures += 1
    if failures > 5:
        priority = 3 (cold)
        delay = 7 days
    else:
        delay based on current priority

If new discovery + offline:
    DROP (don't re-queue — prevents queue pollution with offline IPs)

next_scan_at = now + delay
```

**Progressive port scanning**: When a known Java server (already in the DB, not a new discovery) is found online, adjacent ports are probed (`port+1`, `port-1`). The scan direction is tracked (`direction` field: 0=start, 1=up, -1=down) and continues on subsequent online findings.

---

## Scanner Loop (`packages/scanner/src/scanner_loop.rs`)

The main scanning loop is a `tokio::select!` with three branches:

### Branch 1: Status logging (every 60s)
Logs queue sizes, active task count, and cumulative scan counts per tier.

### Branch 2: Stats flush (every 10s)
Atomically swaps in-memory tier counters (`hot_buffer`, `warm_buffer`, `cold_buffer`, `discoveries_buffer`) to zero and writes them to the `daily_stats` table via `increment_batch_stats`. This prevents DB write storms from per-scan inserts.

### Branch 3: Scan dispatch
1. Calls `scheduler.next_server()`.
2. If `None` (nothing ready), sleeps 2 seconds.
3. If a server is returned, checks `active_tasks >= max_concurrency`:
   - If at capacity, re-queues the server and sleeps 10ms.
   - Otherwise, spawns a `tokio::spawn` task and increments `active_tasks`.

**Scan task** (runs concurrently, up to `TARGET_CONCURRENCY` = 2500):
1. Calls `scanner.scan_server()` — performs the actual network probe (§3).
2. Calls `scheduler.requeue_server()` — updates priority and schedules next scan (§2).
3. Sends the `ScanResult` through an `mpsc::channel` (capacity = `max_concurrency * 2` = 5000).
4. Increments in-memory tier counters.
5. Decrements `active_tasks`.

### Result Batching (background DB writer)

A dedicated `tokio::spawn` task receives `ScanResult`s from the channel and batches them:
- **Buffer size**: 100 results
- **Flush interval**: 1 second
- **Flush condition**: buffer reaches 100 OR 1 second elapses
- **Write method**: `batch_update_results` — single transaction with upserts for servers, history entries (capped at 500 per server), and player records.

This decouples scanning speed from DB write latency. At 100 RPS, the buffer fills every ~1 second, matching the flush interval.

---

## Scanner Engine (`packages/scanner/src/scanner.rs`)

### Rate Limiting

Two independent **token bucket** rate limiters:

| Limiter | Target | Refill Interval | Per-Refill Amount |
|---------|--------|-----------------|-------------------|
| Hot/Warm | `TARGET_RPS` (100) | 10ms | RPS / 100 permits |
| Cold | `TARGET_COLD_RPS` (10) | 10ms | cold_rps / 100 permits |

Each limiter uses a Tokio `Semaphore`:
- Tokens are consumed via `acquire()` + `forget()` (no return).
- A background task refills every 10ms with fractional permit tracking to avoid bursting.
- The semaphore is capped at `target_rps` permits max — prevents burst accumulation beyond 1 second of throughput.

### Concurrency Control

A separate `Semaphore` with `TARGET_CONCURRENCY` (2500) permits limits simultaneous in-flight scans. Each scan task acquires one permit on entry and releases it on drop.

### Scan Execution

For each target:

1. **Exclude check** — `ExcludeManager.is_excluded(ip)` — returns early if matched (no network I/O).
2. **Rate limit** — Acquires permit from hot/warm or cold limiter based on priority.
3. **Concurrency limit** — Acquires semaphore permit.
4. **Network probe**:
   - **Bedrock** (port 19132): RakNet ping via `raknet::ping_server()`
   - **Java** (default 25565): Server List Ping (SLP) via `slp::ping_server()` — handshake, status request, response parse
5. **ASN enrichment** — Looks up the IP in the in-memory `AsnManager` (backed by MaxMind `.mmdb`). Falls back to live `iptoasn.com` fetch if not cached.
6. **Result construction** — Returns a `ScanResult` struct with all parsed fields (online/offline, players, MOTD, version, brand, favicon, ASN, country, player sample).

---

## Login Queue (`packages/scanner/src/login_queue.rs`)

A secondary background process that attempts **actual Minecraft protocol logins** on online servers to detect access restrictions.

### Purpose

SLP (Server List Ping) only returns server metadata. It cannot determine if a server is:
- Premium-only (paid accounts)
- Whitelisted
- IP-banned
- Using a proxy that blocks bots

The login queue performs a full handshake+login attempt with the username "NMCScan" to classify these obstacles.

### Lifecycle

1. **30-minute initial delay** — On first startup, waits 30 minutes to let the SLP scanner populate `version` data across servers. If a previous run already tested servers recently, the delay is skipped.
2. **Cursor-based pagination** — Iterates through ALL online servers in `(ip, port)` order using cursor pagination (500 per batch). The cursor state persists across batch fetches, ensuring every server is visited exactly once per full cycle.
3. **Skip recently tested** — Servers with `last_login_at` within the last hour are skipped.
4. **Rate limiting** — Token bucket at ~60 attempts/second (refill every 16.667µs).
5. **Concurrency** — Max 20 simultaneous login attempts (semaphore).

### Smart Login

Uses `attempt_login_smart` which:
- Starts with the protocol version reported by SLP (if available).
- On disconnect, parses the disconnect reason message to extract the server's actual protocol version.
- Classifies the obstacle: `success`, `premium`, `whitelist`, `banned`, `rejected`, `unreachable`, `timeout`.

### Result Storage

The obstacle type is written to `servers.login_obstacle`, which triggers a PostgreSQL function to auto-compute the `flags` column (comma-separated: `cracked`, `vanilla`, `active`, etc.).

---

## ASN Subsystem (`packages/shared/src/services/asn_fetcher.rs`)

### Data Sources

| Source | Format | Purpose |
|--------|--------|---------|
| MaxMind GeoLite2-ASN | `.mmdb` (local) | Fast ASN lookup for individual IPs |
| MaxMind GeoLite2-Country | `.mmdb` (local) | Country code lookup |
| iptoasn.com | TSV (gzipped) | Full IPv4→ASN range database |
| ipverse/as-metadata | JSON (GitHub) | ASN categorization (hosting/residential/excluded) |

### Initialization

1. Loads local `.mmdb` databases into `maxminddb::Reader` instances.
2. Loads `asns` and `asn_ranges` tables into in-memory `AsnManager` (hash maps for O(1) lookup).
3. If `asn_count < 100` or `range_count < 100`, runs full import from iptoasn.com + ipverse.

### ASN Lookup During Scans

For each scanned IP:
1. Check in-memory `AsnManager` cache first.
2. If miss, query local MaxMind `.mmdb`.
3. If still miss, fetch from `iptoasn.com` live API (rate-limited).
4. Cache the result in `AsnManager` and upsert to `asns`/`asn_ranges` tables.
5. Auto-extract tags from organization name (e.g., "Amazon.com, Inc." → tag: `amazon`).

### Background Refresh

- **Weekly**: Downloads updated `.mmdb` files from public mirrors.
- **On startup if stale**: Re-categorizes "unknown" ASNs using the ipverse dataset (throttled at 500 ASNs per batch with 200ms delays).

---

## Database Interaction Summary

| Operation | Method | Frequency | Batch Size |
|-----------|--------|-----------|------------|
| Queue refill (hot/warm/cold) | `get_servers_for_refill` | Every 5s (per tier) | 5000/3000/2000 |
| Discovery range fetch | `get_ranges_to_scan` | Every 15s | 500 per category |
| Scan result write | `batch_update_results` | Every 1s or 100 results | 100 |
| Stats flush | `increment_batch_stats` | Every 10s | 4 counters |
| Epoch progress update | `update_batch_range_progress` | Every discovery cycle | All processed ranges (single `unnest` query) |
| Login result write | `update_login_result` | Per login attempt | 1 |
| ASN cache upsert | `upsert_asn` | Per scan (cache miss) | 1 |

---

## Capacity & Throughput

| Parameter | Default | Configurable |
|-----------|---------|--------------|
| Scan rate (RPS) | 100 | `TARGET_RPS` |
| Cold scan rate | 10 (10% of RPS) | `TARGET_COLD_RPS` |
| Max concurrent tasks | 2500 | `TARGET_CONCURRENCY` |
| Login queue rate | ~60/sec | Hardcoded |
| Login concurrency | 20 | Hardcoded |
| DB connection pool | 200 | Hardcoded |
| Queue capacity | 100,000 per queue | `MAX_QUEUE_SIZE` constant |
| History cap | 500 entries/server | `MAX_HISTORY_ENTRIES` constant |
| Favicons | Dropped if >2KB | `MAX_FAVICON_SIZE` constant |

**Theoretical daily throughput**: 100 RPS × 86,400 seconds = **8.64 million scans/day**.

At 8.64M scans/day:
- 500K servers on 2h hot cycle (12 scans/day each) = 6M scans — within capacity.
- Discovery scanning (new IPs) consumes a portion of the remaining ~2.64M capacity.

---

## Crash Recovery

The scanner is **stateless by design**. No scan state is persisted in memory — everything is derived from the database on restart:

- **Hot/Warm/Cold queues**: Start empty. Repopulated by `try_refill_queues` based on `last_seen` staleness. No `next_scan_at` is persisted.
- **Discovery progress**: Persisted in `asn_ranges.scan_offset` and `asn_ranges.scan_epoch`. Resumes exactly where it left off.
- **In-flight scans**: On crash, any tasks mid-scan are lost. The target will be rescanned on the next refill cycle (minor duplicate scan, acceptable).
- **Buffered results**: Unflushed results in the 1-second batching buffer are lost. At 100 RPS, this is at most ~100 scan results.
- **ASN manager**: Reloaded from DB + local `.mmdb` files on startup.
