# NMCScan API Documentation

NMCScan exposes a REST API for accessing scan data, server information, and managing access via API keys.

## Authentication

All protected endpoints require an `X-API-Key` header.

- **Master Key**: Configured via the `API_KEY` environment variable. Used by the dashboard to proxy requests. Master key holders can optionally pass `X-User-Id` to impersonate a specific user.
- **User API Keys**: Generated via the dashboard (`/explore/account`) or `POST /api/keys`. These authenticate the request and associate actions with the creating user.

```bash
# Using a user API key
curl -H "X-API-Key: nmc_..." http://localhost:3000/api/stats

# Using the master key with user impersonation
curl -H "X-API-Key: <master_key>" -H "X-User-Id: 42" http://localhost:3000/api/keys
```

## Public Endpoints

No authentication required.

### `GET /api/health`

Returns health status and total server count.

**Response:**
```json
{"status": "ok", "total_servers": 12345}
```

### `GET /api/info`

Returns public configuration info.

**Response:**
```json
{"email": "contact@example.com", "discord": "https://discord.gg/..."}
```

## Data Endpoints

All require authentication.

### `GET /api/stats`

Global scanning statistics.

**Response:**
```json
{
  "total_servers": 50000,
  "online_servers": 12000,
  "total_players": 45000,
  "asn_hosting": 800,
  "asn_residential": 2000,
  "asn_unknown": 150
}
```

### `GET /api/servers`

Lists servers with filtering, sorting, and cursor-based pagination.

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `search` | string | — | Free-text search and/or query DSL (see below) |
| `limit` | int | 50 | Number of results to return |
| `status` | string | — | Filter by status: `online`, `offline` |
| `brand` | string | — | Server software brand (contains match) |
| `version` | string | — | Minecraft version (contains match) |
| `server_type` | string | — | `java` or `bedrock` |
| `country` | string | — | 2-letter country code (exact match) |
| `asn` | string | — | ASN number (exact match) |
| `asn_category` | string | — | `hosting`, `residential`, `education`, `business` |
| `min_players` | int | — | Minimum players online |
| `max_players` | int | — | Maximum players online |
| `min_max_players` | int | — | Minimum server capacity (max_players field) |
| `max_max_players` | int | — | Maximum server capacity |
| `whitelist_prob_min` | float | — | Minimum whitelist probability (0.0–1.0) |
| `sort_by` | string | `players` | Sort field: `players`, `last_seen`, `ip` |
| `sort_order` | string | `desc` | Sort direction: `asc`, `desc` |
| `cursor_ip` | string | — | Cursor: IP of last item from previous page |
| `cursor_players` | int | — | Cursor: player count of last item (when sorting by players) |
| `cursor_last_seen` | datetime | — | Cursor: last_seen of last item (when sorting by last_seen) |

**Response:**
```json
[
  {
    "ip": "1.2.3.4",
    "port": 25565,
    "server_type": "java",
    "status": "online",
    "players_online": 42,
    "players_max": 100,
    "motd": "A Minecraft Server",
    "version": "1.21.4",
    "priority": 1,
    "last_seen": "2026-03-31T12:00:00",
    "consecutive_failures": 0,
    "whitelist_prob": 0.05,
    "asn": "16509",
    "country": "US",
    "favicon": "data:image/png;base64,...",
    "brand": "Paper",
    "asn_org": "Amazon.com, Inc.",
    "asn_tags": ["hosting", "cloud"]
  }
]
```

#### Query DSL

The `search` parameter supports a query DSL for structured filtering. DSL tokens are parsed server-side and extracted from the search text. Remaining text becomes a free-text search across IP, MOTD, and version fields.

**Syntax:**

| Filter | Example | Description |
|--------|---------|-------------|
| `brand:` | `brand:Paper` | Server software (contains match) |
| `version:` | `version:1.21` | Minecraft version (contains match) |
| `country:` | `country:US` | 2-letter country code |
| `status:` | `status:online` | `online` or `offline` |
| `type:` | `type:java` | `java` or `bedrock` |
| `category:` | `category:hosting` | ASN category |
| `asn:` | `asn:16509` | ASN number |
| `players:` | `players:5` | Exactly 5 players online |
| `players:` | `players:>10` | More than 10 players |
| `players:` | `players:<50` | Fewer than 50 players |
| `players:` | `players:10..50` | Between 10 and 50 players |
| `limit:` | `limit:100..500` | Server capacity range |

Quoted values are supported for values with spaces: `brand:"Forge Server"`.

**Examples:**

```bash
# Paper servers in the US with more than 10 players
curl -H "X-API-Key: nmc_..." \
  'http://localhost:3000/api/servers?search=brand:Paper+country:US+players:>10'

# Java servers in hosting ASNs, sorted by last seen
curl -H "X-API-Key: nmc_..." \
  'http://localhost:3000/api/servers?search=type:java+category:hosting&sort_by=last_seen'

# Mixed: DSL filters + free-text search
curl -H "X-API-Key: nmc_..." \
  'http://localhost:3000/api/servers?search=brand:Paper+survival+hardcore'

# DSL filters can also be passed as explicit query params (override DSL values)
curl -H "X-API-Key: nmc_..." \
  'http://localhost:3000/api/servers?brand=Velocity&min_players=5'
```

**Note:** When both DSL tokens and explicit query parameters are provided, explicit parameters take precedence.

#### Cursor-Based Pagination

For efficient pagination through large result sets, use cursor-based pagination instead of offset-based.

1. Make your first request normally (without cursor params).
2. From the response, take the last item's `ip`, `players_online`, and `last_seen` values.
3. Pass them as `cursor_ip`, `cursor_players`, and/or `cursor_last_seen` in the next request.

```bash
# First page
curl -H "X-API-Key: nmc_..." 'http://localhost:3000/api/servers?sort_by=players&limit=50'

# Next page (using cursor from last item: ip=1.2.3.4, players_online=10)
curl -H "X-API-Key: nmc_..." \
  'http://localhost:3000/api/servers?sort_by=players&limit=50&cursor_ip=1.2.3.4&cursor_players=10'
```

The `cursor_players` param is only needed when `sort_by=players`. The `cursor_last_seen` param is only needed when `sort_by=last_seen`. The `cursor_ip` is always needed as a tiebreaker.

### `GET /api/server/{ip}`

Returns detailed information for a specific server. The port can be included as `ip:port` (defaults to 25565).

```bash
curl -H "X-API-Key: nmc_..." http://localhost:3000/api/server/1.2.3.4
curl -H "X-API-Key: nmc_..." http://localhost:3000/api/server/1.2.3.4:19132
```

### `GET /api/server/{ip}/history`

Historical player count data for a server (up to 500 entries, chronological order).

**Response:**
```json
[
  {"timestamp": "2026-03-30T00:00:00", "players_online": 35},
  {"timestamp": "2026-03-31T00:00:00", "players_online": 42}
]
```

### `GET /api/server/{ip}/players`

Recently seen players on a server (up to 100 entries).

**Response:**
```json
[
  {"player_name": "Steve", "player_uuid": "uuid-here", "last_seen": "2026-03-31T12:00:00"}
]
```

### `GET /api/players`

Search for players across all servers.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Player name to search (minimum 3 characters) |

**Response:**
```json
[
  {"ip": "1.2.3.4", "port": 25565, "player_name": "Steve", "last_seen": "2026-03-31T12:00:00"}
]
```

### `GET /api/asns`

Lists Autonomous System Numbers with statistics.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `page` | int | 0 | Page number (0-indexed) |
| `limit` | int | 50 | Results per page |

**Response:**
```json
{
  "items": [
    {"asn": "16509", "org": "Amazon.com", "category": "hosting", "country": "US", "server_count": 500, "tags": ["cloud", "hosting"]}
  ],
  "total": 3000,
  "page": 0,
  "limit": 50
}
```

### `GET /api/asns/{asn}`

Detailed information for a specific ASN.

### `GET /api/exclude`

Returns the current list of excluded IP ranges.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `page` | int | 0 | Page number (0-indexed) |
| `limit` | int | 50 | Results per page |

### `POST /api/exclude`

Adds a new IP range to the exclusion list.

**Body:**
```json
{"network": "192.168.1.0/24", "comment": "Internal network"}
```

### `POST /api/scan/test`

Triggers a manual test scan with known Minecraft servers.

**Body:**
```json
{"count": 10, "region": "us", "quick": true}
```

- `count`: Number of servers to scan (default: 10)
- `region`: Filter by region (`us`, `eu`, `uk`, `au`, `br`, `asia`)
- `quick`: Use a predefined quick test set (10 servers)

**Response:**
```json
{
  "status": "ok",
  "servers_added": 10,
  "servers": [{"ip": "1.2.3.4", "port": 25565, "name": "Hypixel"}]
}
```

## API Key Management

All require authentication. User API keys are scoped to the authenticated user.

### `GET /api/keys`

Lists all active API keys for the authenticated user. The raw key is never returned.

**Response:**
```json
[
  {"id": 1, "name": "My CLI Tool", "created_at": "2026-03-31T12:00:00Z", "last_used_at": "2026-03-31T14:00:00Z"}
]
```

### `POST /api/keys`

Generates a new API key. The raw key is returned **only once** in this response.

**Body:**
```json
{"name": "My CLI Tool"}
```

**Response:**
```json
{"id": 1, "name": "My CLI Tool", "key": "nmc_a1b2c3d4e5f6...", "created_at": "2026-03-31T12:00:00Z", "last_used_at": null}
```

### `DELETE /api/keys/{id}`

Revokes an API key by ID. Only keys owned by the authenticated user can be revoked.

**Response:** `200 OK` on success, `404 Not Found` if key doesn't exist or isn't owned by the user.
