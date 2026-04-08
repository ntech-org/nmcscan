# NMCScan Architecture Guide

## Overview

NMCScan has been split from a monolithic application into a **microservice architecture** with three components:

```
┌─────────────────────┐     ┌──────────────────┐
│   nmcscan-api       │     │ nmcscan-scanner  │
│                     │     │                  │
│  • Web API (Axum)   │     │  • Scanner       │
│  • Migrations       │     │  • Scheduler     │
│  • Login Queue      │     │  • Discovery     │
│  • MV Refresh       │     │  • ASN Import    │
│  • ASN Refresh      │     │  • Epoch Cooldown│
│                     │     │                  │
└─────────┬───────────┘     └────────┬─────────┘
          │                          │
          └──────────┬───────────────┘
                     │
          ┌──────────▼───────────┐
          │   PostgreSQL DB      │
          │                      │
          │  • servers           │
          │  • asn_ranges        │
          │  • asns              │
          │  • daily_stats       │
          │  • api_keys          │
          └──────────────────────┘
```

## Package Structure

```
NMCScan/
├── Cargo.toml              # Workspace definition
├── packages/
│   ├── shared/             # Shared library crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models/     # SeaORM entities, ASN logic
│   │       ├── network/    # Network types & implementations
│   │       ├── repositories/ # Database access layer
│   │       ├── services/   # AsnFetcher, Scheduler
│   │       └── utils/      # ExcludeList, test mode, etc.
│   │
│   ├── api/                # API service crate
│   │   ├── Cargo.toml
│   │   ├── Dockerfile
│   │   └── src/
│   │       ├── main.rs     # API service entry point
│   │       ├── handlers/   # Axum route handlers
│   │       └── login_queue.rs
│   │
│   └── scanner/            # Scanner service crate
│       ├── Cargo.toml
│       ├── Dockerfile
│       └── src/
│           ├── main.rs     # Scanner service entry point
│           ├── scanner.rs  # Network scanning logic
│           └── scanner_loop.rs
│
├── migration/              # Database migrations
├── dashboard/              # Web dashboard (unchanged)
├── exclude.conf            # IP exclusion list
└── honeypots.conf          # Honeypot exclusion list
```

## Building

### Build All Packages
```bash
cargo build --workspace
```

### Build Individual Packages
```bash
# Shared library
cargo build -p nmcscan-shared

# API service
cargo build -p nmcscan-api

# Scanner service
cargo build -p nmcscan-scanner
```

### Build for Release
```bash
cargo build --workspace --release
```

## Running

### Development Mode

**API Service Only** (no scanning):
```bash
cargo run -p nmcscan-api -- \
  --database postgres://nmcscan:nmcscan_secret@localhost:5432/nmcscan \
  --listen-addr 0.0.0.0:3001
```

**Scanner Service Only** (no API):
```bash
cargo run -p nmcscan-scanner -- \
  --database postgres://nmcscan:nmcscan_secret@localhost:5432/nmcscan
```

**Both Services** (full functionality):
```bash
# Terminal 1 - API
cargo run -p nmcscan-api -- --database postgres://... --listen-addr 0.0.0.0:3001

# Terminal 2 - Scanner
cargo run -p nmcscan-scanner -- --database postgres://...
```

### Docker Compose

**Run All Services**:
```bash
docker compose up -d
```

**Run API Only** (stop scanner):
```bash
docker compose up -d nmcscan-api postgres dashboard
docker compose stop nmcscan-scanner
```

**Run Scanner Only** (no API):
```bash
docker compose up -d nmcscan-scanner postgres
docker compose stop nmcscan-api dashboard
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for specific package
cargo test -p nmcscan-shared

# Run with output
cargo test -p nmcscan-shared -- --nocapture
```

## Deployment Scenarios

### Scenario 1: Normal Operation
```yaml
# All services running
services:
  - postgres
  - nmcscan-api
  - nmcscan-scanner
  - dashboard
```

### Scenario 2: Abuse Complaint Response
```bash
# Immediately stop scanning while keeping API/dashboard online
docker compose stop nmcscan-scanner

# API and dashboard remain accessible
# No disruption to users viewing data
```

### Scenario 3: API Maintenance
```bash
# Stop API for maintenance, scanner continues
docker compose stop nmcscan-api dashboard

# Scanner continues collecting data, writing to DB
# Restart API when maintenance complete
```

### Scenario 4: Independent Scaling
```bash
# Multiple scanner instances on different hosts
# Single API instance for data access
# All scanners write to same database
```

## Environment Variables

### API Service
| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level |
| `DATABASE_URL` | - | PostgreSQL connection string |
| `LISTEN_ADDR` | `0.0.0.0:3000` | API bind address |
| `API_KEY` | - | Dashboard authentication key |
| `CONTACT_EMAIL` | - | Contact email for public page |
| `DISCORD_LINK` | - | Discord invite link |
| `EXCLUDE_FILE` | `/app/exclude.conf` | Path to exclusion list |
| `FORCE_ASN_IMPORT` | `false` | Force ASN database re-import |

### Scanner Service
| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level |
| `DATABASE_URL` | - | PostgreSQL connection string |
| `TARGET_RPS` | `100` | Target scans per second |
| `TARGET_CONCURRENCY` | `1000` | Max concurrent scan tasks |
| `TARGET_COLD_RPS` | - | Cold scan rate limit |
| `TEST_MODE` | `false` | Enable test mode |
| `TEST_MAX_SERVERS` | `50` | Max servers in test mode |
| `TEST_SCAN_INTERVAL` | `60` | Test scan interval (seconds) |
| `TEST_REGIONS` | - | Region filter for tests |
| `FORCE_ASN_IMPORT` | `false` | Force ASN database re-import |

## Communication Between Services

Services communicate **only through the database**:

- ✅ **Simple**: No additional protocols to maintain
- ✅ **Reliable**: Database provides persistence & consistency  
- ✅ **Isolatable**: Either service can stop/start independently
- ✅ **Scalable**: Multiple scanner instances can run concurrently

**No direct HTTP/gRPC between services** - both read/write the same database tables.

## Migration from Monolith

### If you have the old monolith running:

1. **Stop the old container**:
   ```bash
   docker compose stop nmcscan
   ```

2. **Run migrations** (if any new ones):
   ```bash
   docker compose up -d postgres
   ```

3. **Start new services**:
   ```bash
   docker compose up -d
   ```

### Data Migration

No data migration needed! Both architectures use the same database schema.

## Troubleshooting

### API won't start
```bash
# Check if database is accessible
docker compose logs postgres

# Check API logs
docker compose logs nmcscan-api
```

### Scanner not scanning
```bash
# Check scanner logs
docker compose logs nmcscan-scanner

# Verify ASN data is loaded
docker compose exec nmcscan-api nmcscan-api --help
```

### Stopping scanner during abuse complaint
```bash
# Immediate stop - API keeps running
docker compose stop nmcscan-scanner

# Verify API is still responsive
curl http://localhost:3001/api/health
```

## Development Workflow

### Adding New Features

1. **Database changes**: Add migration in `migration/`
2. **Shared logic**: Add to `packages/shared/src/`
3. **API endpoints**: Add handler in `packages/api/src/handlers/`
4. **Scanning logic**: Add to `packages/scanner/src/`

### Testing Changes

```bash
# 1. Build all packages
cargo build --workspace

# 2. Run tests
cargo test --workspace

# 3. Run services locally
cargo run -p nmcscan-api -- ...
cargo run -p nmcscan-scanner -- ...
```

## Architecture Decisions

### Why Split Services?

1. **Safety**: Stop scanner without affecting API/dashboard
2. **Isolation**: Bugs in scanner don't crash API
3. **Flexibility**: Run API-only mode for viewing data
4. **Scalability**: Multiple scanners on different hosts
5. **Maintainability**: Clear separation of concerns

### Why Shared Library?

- Prevents code duplication
- Ensures consistent data models
- Simplifies testing
- Single source of truth for repositories

### Why Database Communication?

- Simple and reliable
- No additional infrastructure needed
- Persists state across restarts
- Easy to debug and monitor

## License

MIT - See LICENSE file
