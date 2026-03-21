# NMCScan Deployment Guide

NMCScan is designed to be deployed on a VPS using Docker.

## Prerequisites
- Docker and Docker Compose installed on your VPS.
- An `exclude.conf` file in the same directory as `docker-compose.yml`.

## Deployment Steps

1. **Clone the repository** (or copy the files to your VPS).
2. **Create a `.env` file** (optional but recommended):
   ```bash
   API_KEY=your_secure_random_key_here
   RUST_LOG=info
   ```
3. **Set up directories**:
   ```bash
   mkdir -p data
   touch exclude.conf # Ensure it has content or copy from repo
   ```
4. **Build and start**:
   ```bash
   docker compose up -d --build
   ```

## Security & Authentication

All API endpoints (except `/health` and the dashboard root `/`) are protected by the `X-API-Key` header.

### Accessing the API via CLI
```bash
curl -H "X-API-Key: your_secure_random_key_here" http://your-vps-ip:3000/stats
```

### Accessing the Dashboard
Since the dashboard is a pre-built static asset, you may need a browser extension to inject the `X-API-Key` header or use a reverse proxy (like Nginx) to handle authentication or header injection if the UI doesn't natively prompt for it.

## Volumes & Persistence
- `/app/data`: Contains `nmcscan.db`. This directory is mounted to `./data` on your host for persistence across container restarts.
- `/app/exclude.conf`: Mounted as read-only. Update this file on the host to change excluded networks.

## Monitoring Logs
```bash
docker compose logs -f
```
