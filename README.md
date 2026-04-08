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

## 🏗️ Architecture

```
                     ┌─────────────────┐
                     │   exclude.conf   │
                     └────────┬────────┘
                              │
    ┌─────────────┐    ┌──────▼───────┐    ┌──────────────────────┐
    │ ASN Fetcher  │───▶│  Scheduler   │    │  Scanner             │
    │ MaxMind +    │    │  Hot/Warm/   │───▶│  SLP (Java)          │
    │ iptoasn.com  │    │  Cold/Disc.  │    │  RakNet (Bedrock)    │
    │ ipverse      │    └──────┬───────┘    │  Login protocol      │
    └─────────────┘           │             └──────────┬───────────┘
                              │                        │
    ┌─────────────┐    ┌──────▼───────┐                │
    │ Login Queue  │    │   Axum API   │◀───────────────┘
    │ 60/sec       │    │  Port 3000   │
    └─────────────┘    └──────┬───────┘     ┌──────────────────┐
                              │             │   PostgreSQL     │
                       ┌──────▼───────┐     │   + pg_trgm      │
                       │  Dashboard    │     │   + mat. views   │
                       │  SvelteKit    │     └──────────────────┘
                       └──────────────┘
```

## 🚀 Quick Start

### Prerequisites

- **Rust** 1.75+ (2021 edition)
- **PostgreSQL** 16+ with `pg_trgm` extension
- **Bun** (for the dashboard frontend, optional)

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
- **nmcscan** — The Rust scanner binary (API on port 3000)
- **dashboard** — SvelteKit frontend

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

The REST API runs on port 3000 by default. Full documentation in [API.md](API.md).

### Public (no auth)

| Endpoint | Description |
|----------|-------------|
| `GET /api/health` | Health check with server count |
| `GET /api/info` | Contact info (email, Discord) |

### Protected (auth required)

| Endpoint | Description |
|----------|-------------|
| `GET /api/stats` | Global stats (servers, players, ASN breakdown) |
| `GET /api/servers` | List servers with DSL search, filters, cursor pagination |
| `GET /api/server/{ip}` | Server details |
| `GET /api/server/{ip}/history` | Historical player counts |
| `GET /api/server/{ip}/players` | Players seen on a server |
| `GET /api/players?name=X` | Search players across all servers |
| `GET /api/asns` | Paginated ASN list |
| `GET /api/exclude` | Paginated exclude list |
| `POST /api/exclude` | Add exclusion |
| `POST /api/scan/test` | Trigger test scan |
| `GET /api/scan/progress` | Scan progress |
| `GET /api/login-queue/status` | Login queue stats |
| `POST /api/login-queue/trigger` | Trigger single login test |
| `GET /api/keys` | List API keys |
| `POST /api/keys` | Create API key |
| `DELETE /api/keys/{id}` | Revoke API key |

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

NMCScan uses PostgreSQL with SeaORM with the following schema:

| Table | Purpose |
|-------|---------|
| `servers` | Server records with INET IP, SMALLINT port, auto-computed flags |
| `server_players` | Players seen on servers |
| `server_history` | Historical player counts (capped at 500 per server) |
| `asns` | ASN records with org, category, country, tags |
| `asn_ranges` | CIDR ranges mapped to ASNs with scan progress tracking |
| `daily_stats` | Daily scan counts per tier |
| `api_keys` | User-generated API keys (SHA-256 hashed) |
| `minecraft_accounts` | Stored Minecraft accounts |
| `users` / `accounts` / `sessions` | Auth.js OAuth tables |

### Materialized Views

- **`asn_stats`** — ASN with server counts (refreshed every 5 minutes)
- **`global_stats`** — Aggregate server/player counts

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

## 📁 Project Structure

```
NMCScan/
├── src/
│   ├── main.rs                    # Entry point, CLI, orchestration
│   ├── network/
│   │   ├── slp.rs                 # Java Server List Ping protocol
│   │   ├── raknet.rs              # Bedrock RakNet unconnected ping
│   │   ├── login.rs               # Offline-mode login protocol
│   │   └── scanner.rs             # Rate-limited concurrent scanner
│   ├── services/
│   │   ├── scheduler.rs           # Hot/Warm/Cold/Discovery queues
│   │   ├── login_queue.rs         # Background login testing
│   │   └── asn_fetcher.rs         # ASN data management
│   ├── handlers/
│   │   ├── mod.rs                 # Axum router + API endpoints
│   │   ├── api_keys.rs            # API key CRUD
│   │   └── minecraft_accounts.rs  # Minecraft account CRUD
│   ├── models/
│   │   ├── asn.rs                 # ASN categories and manager
│   │   └── entities/              # SeaORM entity definitions
│   ├── repositories/              # Data access layer
│   └── utils/
│       ├── exclude.rs             # Exclude list manager
│       └── query_parser.rs        # DSL query parser
├── migration/                     # SeaORM migrations (8 total)
├── dashboard/                     # SvelteKit frontend
├── compose.yaml                   # Docker Compose
├── Dockerfile                     # Multi-stage Rust build
├── exclude.conf                   # IP exclusion list
├── Cargo.toml
└── README.md
```

## 📝 License

MIT License — see [LICENSE](LICENSE) for details.
