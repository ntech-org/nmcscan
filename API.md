# NMCScan API Documentation

NMCScan exposes a REST API for accessing scan data, server information, and managing access via API keys. 

## Authentication

All protected endpoints require an `X-API-Key` header.
- **Master Key**: Used by the dashboard to proxy requests (configured via the `API_KEY` environment variable).
- **User API Keys**: Users can generate their own API keys via the dashboard/API. These keys will authenticate the request and associate any actions with that user.

Example:
```bash
curl -H "X-API-Key: nmc_..." http://localhost:3000/api/stats
```

## Global Endpoints

### `GET /api/health`
Returns the health status of the API and the total number of servers tracked.
- **Auth required:** No
- **Response:** `{"status": "ok", "total_servers": 12345}`

### `GET /api/info`
Returns public configuration info like target RPS, contact email, and test mode status.
- **Auth required:** No

## Data Endpoints (Auth Required)

### `GET /api/stats`
Returns global scanning statistics (total servers, online, offline, hot/warm/cold counts).

### `GET /api/servers`
Lists servers based on query parameters.
- **Query parameters:**
  - `page`: Page number (default 1)
  - `limit`: Results per page (default 50)
  - `search`: Search by IP, hostname, or MOTD
  - `version`: Filter by Minecraft version
  - `modded`: Filter by modded status (true/false)
  - `asn`: Filter by Autonomous System Number
  - `country`: Filter by Country Code
  - `online`: Filter by online status (true/false)

### `GET /api/server/{ip}`
Returns detailed information for a specific server by IP address.

### `GET /api/server/{ip}/history`
Returns historical scan data for a specific server.

### `GET /api/server/{ip}/players`
Returns the list of recently seen players for a specific server.

### `GET /api/players`
Search for players across all servers.
- **Query parameters:** `name` (required)

### `GET /api/asns`
Lists Autonomous System Numbers (ASNs) and their statistics.

### `GET /api/asns/{asn}`
Returns detailed information for a specific ASN.

### `GET /api/exclude`
Returns the current list of excluded IP ranges.

### `POST /api/exclude`
Adds a new IP range to the exclusion list.
- **Body:** `{"network": "192.168.1.0/24", "comment": "Internal network"}`

### `POST /api/scan/test`
Triggers a manual scan test (if test mode is enabled).

## API Key Management (Auth Required)

These endpoints allow users to manage their own API keys.

### `GET /api/keys`
Lists metadata for all active API keys owned by the authenticated user.
- **Response:** `[{"id": 1, "name": "My Tool", "created_at": "...", "last_used_at": "..."}]`

### `POST /api/keys`
Generates a new API key. The raw key is returned only once.
- **Body:** `{"name": "My CLI Tool"}`
- **Response:** `{"id": 1, "name": "My CLI Tool", "key": "nmc_..."}`

### `DELETE /api/keys/{id}`
Revokes an API key by ID.
- **Response:** `200 OK`
