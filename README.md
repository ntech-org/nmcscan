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
