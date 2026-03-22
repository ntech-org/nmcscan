# NMCScan Deployment Guide

NMCScan consists of a Rust backend (API) and a SvelteKit frontend (Dashboard). They are decoupled for maximum performance and reliability.

## 🏗️ Architecture
- **Backend**: Rust binary running in Docker. Port 3000.
- **Frontend**: Statically generated SvelteKit app. Served by Caddy on the host.

## 🚀 Deployment Steps

### 1. Backend (Docker)
1. Prepare your environment:
   ```bash
   # Create data directory
   mkdir -p data
   # Ensure exclude.conf exists
   touch exclude.conf
   ```
2. Update `compose.yaml` or `.env` with your `API_KEY`.
3. Start the backend:
   ```bash
   docker compose up -d --build
   ```

### 2. Frontend (Static Build)
1. Go to the dashboard directory:
   ```bash
   cd dashboard
   ```
2. Build the static site (ensure you have Node/Bun installed):
   ```bash
   # Set the API URL for the frontend
   export PUBLIC_API_URL="https://your-domain.com"
   bun install
   bun run build
   ```
   This will generate a `build/` directory with all static files.

### 3. Caddy Configuration
Serve the static files and proxy the API using Caddy on your host machine.

Example `Caddyfile`:
```caddy
your-domain.com {
    # 1. Serve the frontend statically
    root * /path/to/NMCScan/dashboard/build
    file_server {
        index index.html
    }

    # 2. Handle SvelteKit client-side routing
    try_files {path} /index.html

    # 3. Proxy API requests to the Rust backend
    handle_path /api/* {
        reverse_proxy localhost:3000
    }
    
    # 4. Fallback for public info/health (if needed)
    handle /info {
        reverse_proxy localhost:3000
    }
    handle /health {
        reverse_proxy localhost:3000
    }

    # Security headers
    header {
        # Enable CORS if frontend/backend are on different subdomains
        Access-Control-Allow-Origin *
        Access-Control-Allow-Methods "GET, POST, OPTIONS"
        Access-Control-Allow-Headers "Content-Type, X-API-Key"
    }
}
```

## 🔒 Security & Authentication
The Dashboard will prompt you for your `API_KEY` on first visit and save it in `localStorage`.
All API requests (except `/health` and `/info`) require the `X-API-Key` header.

## 📂 Persistence
- **Database**: Stored in `./data/nmcscan.db`.
- **Exclusions**: Update `./exclude.conf` on the host. The scanner reloads this file automatically or via the dashboard API.
