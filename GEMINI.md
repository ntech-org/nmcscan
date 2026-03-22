# GEMINI.md - NMCScan Project Context

## Project Overview
NMCScan is a high-performance, ethical Minecraft Java Edition server scanner. It utilizes a priority-based scheduling system to scan IP ranges, categorize servers, and provide a web-based dashboard for monitoring and analysis.

### Core Technologies
- **Backend:** Rust (Tokio, Axum, SQLx/SQLite, Serde)
- **Frontend:** SvelteKit (Static Adapter), TypeScript, TailwindCSS v4
- **Database:** SQLite (managed via SQLx)
- **Infrastructure:** Docker, Docker Compose

### Architecture
- **Scanner:** Performs Server List Ping (SLP) on targets with rate and concurrency limiting.
- **Scheduler:** Manages three priority tiers:
  - **Tier 1 (Hot):** Online servers, scanned frequently (every few hours).
  - **Tier 2 (Warm):** Known hosting ASN ranges, scanned weekly.
  - **Tier 3 (Cold):** Residential IPs and high-failure servers, scanned monthly.
- **ASN Fetcher:** Dynamically identifies hosting vs. residential IP ranges using ASN data.
- **API:** Axum-based REST API for stats, server lists, and dashboard integration.
- **Safety/Ethics:** Strict enforcement of `exclude.conf` (US Military, Universities, etc.) and connection rate limiting (~100 RPS).

## Building and Running

### Backend (Rust)
```bash
# Build release binary
cargo build --release

# Run with custom configuration
./target/release/nmcscan --database data/nmcscan.db --exclude-file exclude.conf
```

### Frontend (Dashboard)
```bash
cd dashboard
bun install
bun run build # Outputs to dashboard/build
```
*Note: The frontend is built as a static site and is intended to be served by the backend or a reverse proxy.*

### Docker Deployment
```bash
# Build and start all services
docker compose up --build
```

## Development Conventions

### Backend
- **Async Runtime:** Uses `tokio` for high-concurrency I/O.
- **Database:** SQLite with WAL mode enabled for concurrent read/write performance.
- **Safety:** Always verify IP ranges against the `ExcludeManager` before scanning.
- **Tracing:** Use the `tracing` crate for structured logging; levels are configurable via `RUST_LOG`.

### Frontend
- **Framework:** Svelte 5 with SvelteKit.
- **Styling:** TailwindCSS v4 (using the `@tailwindcss/vite` plugin).
- **State Management:** Uses Svelte runes (`$state`, `$derived`).

## Key Files and Directories
- `src/main.rs`: Application entry point and orchestrator.
- `src/scanner.rs`: Logic for Minecraft Server List Ping (SLP).
- `src/scheduler.rs`: Priority queue and scanning logic.
- `src/db.rs`: Database schema and SQLx queries.
- `src/api.rs`: Axum router and API endpoint handlers.
- `dashboard/src/`: SvelteKit frontend source code.
- `exclude.conf`: Required configuration for safe/ethical scanning.
- `nmcscan.db`: SQLite database file (typically stored in `data/`).

## Environment Variables
- `API_KEY`: Optional key for dashboard authentication.
- `TARGET_RPS`: Target requests per second (default: 100).
- `CONTACT_EMAIL`: Public contact email shown on the dashboard.
- `DISCORD_LINK`: Public Discord link for the community.
- `TEST_MODE`: When `true`, only scans a predefined list of known servers.
