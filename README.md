# NMCScan

[![Rust](https://github.com/ntech-org/nmcscan/actions/workflows/rust.yml/badge.svg)](https://github.com/ntech-org/nmcscan/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

High-performance, ethical Minecraft Java & Bedrock Edition server scanner with ASN intelligence, priority-based scheduling, and a real-time web dashboard.

## 🌟 Features

- **Dual-protocol scanning** — Java Server List Ping (SLP) with modern (1.7+) and legacy (1.6 and below) support, plus Bedrock RakNet unconnected ping
- **ASN intelligence** — Auto-categorizes IPs into Hosting, Residential, or Excluded using MaxMind GeoLite2, iptoasn.com, and ipverse community data
- **Priority scheduling** — Hot (online servers, every 2h), Warm (hosting ASNs, every 24h), Cold (residential, every 7 days), and Discovery (new IP ranges)
- **Deterministic discovery** — Hash-based IP shuffle across ASN CIDR ranges with no bitset needed; same CIDR + epoch always produces the same IPs
- **Offline-mode login testing** — Detects cracked, premium, whitelisted, and banned servers with smart protocol version extraction from disconnect messages
- **Progressive port scanning** — Automatically scans adjacent ports when a Java server is found online
- **Auto brand detection** — Vanilla, Forge, Fabric, NeoForge, Paper, Spigot, Purpur, BungeeCord, Velocity, and more
- **Full-text search** — DSL query language with cursor-based pagination across millions of servers
- **Safety-first** — Strict exclude list (US Military, Universities, complainants), rate limiting, and concurrency controls
- **REST API** — Axum-powered HTTP API with gzip compression, CORS, and API key management
- **Web dashboard** — SvelteKit frontend with real-time stats and server browsing

## 🛡️ Safety

NMCScan is designed for ethical operation:

| Safeguard | Detail |
|-----------|--------|
| **Exclude list** | `exclude.conf` blocks US Military (/8 blocks), UK Janet, Universities, UCEPROTECT, and complaining IPs — checked *before* every connection |
| **Rate limiting** | Token bucket at 100 RPS default, separate cold IP limiter at 10 RPS |
| **Concurrency limit** | Max 2,500 simultaneous tasks via semaphore |
| **No exploitation** | Login testing uses fixed username "NMCScan" — no password cracking or auth exploitation |
| **Pure protocol** | Only standard SLP/RakNet ping and login handshake — no exploits |
| **Hot-reloadable exclusions** | New exclusions via API without restart |

**Note: Always ensure you have permission to scan IP ranges and respect local laws and regulations. Use the exclude list to block any sensitive ranges. This depends on hosting provider policies. We strongly advise to scan with caution, most providers will KILL your service when abuse reports come in.*

## 🚀 Quick Start

### Prerequisites

- **Rust** Latest Rust recommend (1.x) (2021 edition)
- **PostgreSQL** 16+ with `pg_trgm` extension
- **Bun** (for the dashboard frontend, optional - **recommended**)

### Docker Compose (Recommended)

```bash
git clone https://github.com/ntech-org/nmcscan.git
cd nmcscan

# Copy and configure environment
cp .env.example .env
# Edit .env with your settings (API_KEY, DATABASE_URL, etc.)

# Build and start all services
docker compose up -d --build
```

This starts three containers:
- **postgres** — PostgreSQL 16 with tuned settings
- **nmcscan-scanner** — The Rust scanner binary (depends on API to be ready)
- **nmcscan-api** — The Rust API server: may be ran without scanner to just serve the DB
- **dashboard** — SvelteKit frontend - exposes port 3000 with the dashboard, proxied to the API

### Manual Setup

```bash
# 1. Clone and configure
git clone https://github.com/ntech-org/nmcscan.git
cd nmcscan
cp .env.example .env

# 2. Create PostgreSQL database
createdb nmcscan
export DATABASE_URL="postgres://user:pass@localhost/nmcscan"

# 3. Build
cargo build --release

# 4. Run
./target/release/nmcscan --database "$DATABASE_URL"
```

### Configuration

All settings can be passed via CLI arguments, environment variables, or `.env` file:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://nmcscan:nmcscan_secret@localhost:5432/nmcscan` | PostgreSQL connection string |
| `TARGET_RPS` | `100` | Scan connections per second |
| `TARGET_CONCURRENCY` | `2500` | Max simultaneous scan tasks |
| `TARGET_COLD_RPS` | `10` | Separate rate limit for cold/discovery IPs |
| `API_KEY` | *(none)* | Master API key for dashboard authentication |
| `TEST_MODE` | `false` | Only scan known servers (development) |
| `FORCE_ASN_IMPORT` | `false` | Force full ASN database re-import on startup |
| `EXCLUDE_FILE` | `exclude.conf` | Path to the exclude list |
| `LISTEN_ADDR` | `0.0.0.0:3000` | API listen address |
| `RUST_LOG` | `info` | Log level (`debug`, `info`, `warn`, `error`) |
| `CONTACT_EMAIL` | *(none)* | Public contact email (shown on dashboard) |
| `DISCORD_LINK` | *(none)* | Public Discord invite link |

### CLI Arguments

```bash
./target/release/nmcscan --help

Usage: nmcscan [OPTIONS]

Options:
  -t, --test-mode              Only scan known servers
      --test-max-servers <N>   Max servers in test mode [default: 50]
      --quick-test             Quick test with ~5 servers
      --test-interval <SECS>   Scan interval in test mode [default: 60]
      --test-regions <REGIONS> Region filter (us, eu, uk, au, br, asia)
  -l, --log-level <LEVEL>      Log level [default: info]
  -d, --database <URL>         PostgreSQL connection string
  -e, --exclude-file <PATH>    Exclude list path [default: exclude.conf]
      --target-rps <RPS>       Connections per second [default: 100]
      --target-concurrency <N> Max simultaneous tasks [default: 2500]
      --target-cold-rps <RPS>  Rate limit for cold IPs [default: 10]
      --listen-addr <ADDR>     API listen address [default: 0.0.0.0:3000]
      --api-key <KEY>          Master API key
      --contact-email <EMAIL>  Public contact email
      --discord-link <URL>     Public Discord link
      --force-asn-import       Force full ASN re-import on startup
  -h, --help                   Print help
```

## 🌐 ASN Intelligence

NMCScan uses three data sources to categor every IP:

| Source | Purpose |
|--------|---------|
| **MaxMind GeoLite2** | Fast local ASN + country lookup via `.mmdb` databases |
| **iptoasn.com** | Full IPv4 ASN range database for global discovery |
| **ipverse/as-metadata** | Community-maintained ASN categorization (hosting/isp/business/education) |

### ASN Categories

| Category | Description | Scan Frequency |
|----------|-------------|----------------|
| **Hosting** | Cloud providers, VPS, data centers | Every 2 hours |
| **Residential** | Home ISPs | Every 7 days |
| **Excluded** | Military, government, education, sensitive infrastructure | Never scanned |
| **Unknown** | Unclassified (default until categorized) | Also excluded |

### Forcing ASN Re-import

When adding new providers or refreshing categorization:

```bash
# CLI flag
./target/release/nmcscan --force-asn-import

# Or environment variable
FORCE_ASN_IMPORT=true ./target/release/nmcscan
```

## 📡 API Endpoints

The REST API runs on port 3000 by default. See [API.md](API.md).

### Query DSL

The `/api/servers` endpoint supports a powerful search DSL:

```
GET /api/servers?search=brand:Paper country:US players:>10 category:hosting flag:cracked
```

| Filter | Syntax | Examples |
|--------|--------|----------|
| Brand | `brand:X` | `brand:Paper`, `brand:"Forge Server"` |
| Country | `country:X` | `country:US`, `country:DE` |
| Players | `players:N`, `players:>N`, `players:10..50` | `players:>10`, `players:0..5` |
| Category | `category:X` | `category:hosting`, `category:residential` |
| Type | `type:X` | `type:java`, `type:bedrock` |
| Status | `status:X` | `status:online`, `status:offline` |
| Flag | `flag:X` | `flag:cracked`, `flag:vanilla`, `flag:active` |
| Login | `login:X` | `login:success`, `login:premium`, `login:whitelist` |

Anything else: searches description/MOTD, incase-sensitive.

## 🗄️ Database

NMCScan uses PostgreSQL with SeaORM with the following schema.

## 📊 Performance

| Metric | Default | Configurable |
|--------|---------|--------------|
| Scan rate | 100 RPS | `TARGET_RPS` |
| Concurrency | 2,500 tasks | `TARGET_CONCURRENCY` |
| Cold IP rate | 10 RPS | `TARGET_COLD_RPS` |
| Login queue | 60/sec, 20 concurrent | Built-in |
| Connection timeout | 10s (SLP), 5s (connect) | Built-in |
| DB pool | 100 connections | PostgreSQL |

## 🚢 Deployment

### Production with Docker + Caddy

See [deployment.md](deployment.md) for a full production deployment guide using Docker Compose for the backend and Caddy as a reverse proxy serving the SvelteKit dashboard.

### Resource Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 1 core | 4+ cores |
| RAM | 1 GB | 4+ GB |
| Disk | 10 GB | 50+ GB (PostgreSQL WAL) |
| Network | 100 Mbps | 1 Gbps |

*NOTE: I personally run NMCScan on a 1 vCore Ryzen VPS with 4 GB of RAM and a 1Gbps network connection, and it performs well at the default 100 RPS and 2,500 concurrency. Adjust the `TARGET_RPS` and `TARGET_CONCURRENCY` settings based on your server's capabilities and network conditions.*

## 📝 License

MIT License — see [LICENSE](LICENSE) for details.
