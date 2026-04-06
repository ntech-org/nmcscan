# NMCScan - Minecraft Server Scanner

High-performance, safe, and ethical Minecraft Java Edition server scanner with priority-based scheduling.

## 🛡️ Safety Features

- **Strict Exclude List**: Parses `exclude.conf` to avoid US Military ranges, Universities, and complaining IPs
- **Rate Limiting**: ~100 connections/second maximum
- **Concurrency Limiting**: Max 200 simultaneous tasks
- **Timeout**: 3 seconds per connection
- **Ethical**: Uses only standard Server List Ping (SLP), no authentication attempts

## 🏗️ Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ ExcludeList │────▶│   Scanner    │────▶│   SQLite    │
└─────────────┘     └──────────────┘     └─────────────┘
                           │
                           ▼
                    ┌──────────────┐     ┌─────────────┐
                    │  Scheduler   │────▶│   Axum API  │
                    │ (Hot/Warm/   │     │  (Port 3000)│
                    │   Cold)      │     └─────────────┘
                    └──────────────┘
```

## 🚀 Quick Start

### Build

```bash
cargo build --release
```

### Run

```bash
# Copy exclude.conf if not present
cp scanner-old/exclude.conf .

# Create database file (first time only)
touch nmcscan.db

# Run the scanner
./target/release/nmcscan
```

### Configuration

- **Database**: `nmcscan.db` (SQLite, auto-created)
- **Exclude List**: `exclude.conf` (required for safety)
- **Web API**: `http://0.0.0.0:3000`
- **Log Level**: Set `RUST_LOG=debug` for verbose output

## 📡 API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | HTML dashboard |
| `GET /health` | Health check: `{"status": "ok", "total_servers": 123}` |
| `GET /servers?limit=50&status=online` | List servers (ordered by players) |
| `GET /server/{ip}` | Server details |

## 🧠 Priority Algorithm

- **Tier 1 (Hot)**: Online servers, last seen < 4 hours
- **Tier 2 (Warm)**: Known hosting ASN ranges, not scanned in 7 days
- **Tier 3 (Cold)**: High-failure servers (>5 failures), very slow scan

## 🌐 ASN Intelligence System

NMCScan uses a sophisticated ASN (Autonomous System Number) classification system to prioritize scanning:

### ASN Categories

- **Hosting**: Cloud providers, VPS hosts, data centers (scanned frequently)
- **Residential**: Home ISP networks (scanned rarely)
- **Excluded**: Military, government, education, sensitive infrastructure (NEVER scanned)
- **Unknown**: Unclassified (default until categorized)

### ASN Data Sources

1. **MaxMind GeoLite2**: Local ASN and country databases
2. **iptoasn.com**: Full IPv4 ASN range database
3. **ipverse/as-metadata**: Community-maintained ASN categorization (hosting/isp/business/education_research)

### Recategorization

The system automatically recategorizes unknown ASNs on startup if:
- No ASN data was updated in the last 7 days, OR
- More than 50% of ASNs are still uncategorized

**Force a full ASN re-import** (useful when adding new providers like ipverse):

```bash
# Using CLI flag
./target/release/nmcscan --force-asn-import

# Or via environment variable
FORCE_ASN_IMPORT=true ./target/release/nmcscan

# Or in docker compose
FORCE_ASN_IMPORT=true docker compose up
```

This will:
1. Download the latest ipverse category map
2. Download the full iptoasn.com database
3. Import all ASN ranges with proper categorization
4. Recategorize all previously unknown ASNs

**Monitor ASN statistics**:

```sql
-- Check ASN category distribution
SELECT category, COUNT(*) as count 
FROM asns 
GROUP BY category;

-- Check how many ASNs are still unknown
SELECT 
    COUNT(*) as total,
    COUNT(CASE WHEN category = 'unknown' THEN 1 END) as unknown,
    ROUND(100.0 * COUNT(CASE WHEN category = 'unknown' THEN 1 END) / COUNT(*), 2) as pct_unknown
FROM asns;
```

## 📊 Database Schema

```sql
CREATE TABLE servers (
    ip TEXT PRIMARY KEY,
    port INTEGER DEFAULT 25565,
    status TEXT DEFAULT 'unknown',
    players_online INTEGER DEFAULT 0,
    players_max INTEGER DEFAULT 0,
    motd TEXT,
    version TEXT,
    priority INTEGER DEFAULT 2,
    last_seen TIMESTAMP,
    consecutive_failures INTEGER DEFAULT 0,
    whitelist_prob REAL DEFAULT 0.0
);
```

## ⚙️ Performance Tuning

For low-resource VPS (1 vCPU, 1GB RAM):

- Already optimized with `lto = true` and `strip = true`
- SQLite WAL mode for concurrent reads
- Rate limiting prevents network saturation

## 📝 License

MIT License
