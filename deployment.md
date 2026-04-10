# NMCScan Deployment Guide

NMCScan consists of a Rust backend (API), Rust Scanner and a SvelteKit frontend (Dashboard). They are decoupled for maximum performance and reliability.

## 🏗️ Architecture
- **Backend**: 
  - **API**: Axum-based REST API for managing scans, exclusions, and providing data to the frontend. Can run without scanner to serve just the DB and API.
  - **Scanner**: Concurrent Rust application that performs the actual scanning of Minecraft servers, depends on API to be ready before starting.
- **Frontend**: Statically generated SvelteKit app. Served by Caddy on the host.

## 🚀 Deployment Steps

## !!! STRONGLY RECOMMENDED !!!
We strongly recommend that you set up a static site for the rDNS of your server, with information about the project and contact details. This can help reduce abuse complaints and provide transparency to network administrators who may see scanning activity from your IP.

## Docker (Recommended)

1. Make sure Docker and Docker Compose are installed on your system.
2. Update `compose.yaml` or `.env` with your `API_KEY`.
3. Start the entire stack with:
   ```bash
   docker compose up -d --build
   ```
This will build and run both the backend and frontend in separate containers. Dashboard will be at `http://localhost:3000` or your port of choice.
API is proxied through the frontend, so you don't need to worry about CORS or separate API hosting.


## Manual
### 1. Backend
1. Prepare your environment:
   ```bash
   # Create data directory
   mkdir -p data
   ```
### Ensure exclude.conf exists, PLEASE USE THE ONE FROM THE REPO, DO NOT CREATE AN EMPTY ONE:
creating an empty one MAY cause the scanner to scan excluded ranges, which is ILLEGAL and may cause abuse complaints. many IPv4 ranges are
owned by governments, universities, and private companies that do not want to be scanned. using the provided exclude.conf ensures you are not scanning these ranges.

2. Update `compose.yaml` or `.env` with your `API_KEY`.
3. Start the backend:
   ```bash
   docker compose up -d --build
   ```

### 2. Frontend (with Bun)
1. Go to the dashboard directory:
   ```bash
   cd dashboard
   ```
2. Build the dashboard (ensure you have Node/Bun installed):
   ```bash
   # Set the API URL for the frontend, adjust if your backend is on a different domain or port (optional if same origin)
   export PUBLIC_API_URL="https://your-domain.com"
   bun install
   bun run build
   ```
3. Run the dashboard in production mode:
   ```bash
   bun run preview
   ```

### Webserver - Caddy Configuration
We strongly recommend caddy for its ease of use, automatic HTTPS, and built-in security features. You can also use Nginx or Apache if you prefer, but Caddy's configuration is simpler for this use case.

Serve the dashboard; Caddy will proxy API requests to the frontend, API requests will be proxied to the backend via the frontend.

Example `Caddyfile`:
```caddy
your-domain.com {
    # Proxy everything to the local app
    reverse_proxy localhost:3000

    # Security Headers (Applied to all responses from the app)
    header {
        X-Frame-Options DENY
        X-Content-Type-Options nosniff
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        Referrer-Policy strict-origin-when-cross-origin
        # Remove server header to hide tech stack
        -Server
    }

    # Logging (optional, but recommended for monitoring and debugging)
    log {
        output file /var/log/caddy/nmcscan_public.log
    }
}

```
