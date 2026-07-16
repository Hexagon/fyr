# API_REFERENCE.md — Complete API Endpoints

## Base URL

All endpoints are relative to `http://localhost:8080` (default)

```
http://localhost:8080/api/...
```

## Status & Configuration

### GET /api/status

Server status and content inventory count.

**Request**:
```bash
curl http://localhost:8080/api/status
```

**Response** (200 OK):
```json
{
  "version": "0.1.0",
  "status": "running",
  "data_dir": "/home/user/data",
  "content_count": {
    "maps": 5,
    "books": 23,
    "poi": 12
  }
}
```

**Use Case**: Periodically check server health and content count.

---

### GET /api/config

Server configuration and directory paths.

**Request**:
```bash
curl http://localhost:8080/api/config
```

**Response** (200 OK):
```json
{
  "data_dir": "/home/user/data",
  "server_host": "127.0.0.1",
  "server_port": 8080,
  "directories": {
    "maps": "/home/user/data/maps",
    "books": "/home/user/data/books",
    "poi": "/home/user/data/poi",
    "inbox": "/home/user/data/inbox"
  }
}
```

**Use Case**: Discover server configuration for programmatic access.

---

## Content Listing

### GET /api/content/maps

List all available map files.

**Request**:
```bash
curl http://localhost:8080/api/content/maps
```

**Response** (200 OK):
```json
[
  {
    "id": "europe",
    "name": "europe.pmtiles",
    "content_type": "map",
    "file_path": "/home/user/data/maps/europe.pmtiles",
    "file_size": 1073741824,
    "checksum": null,
    "created_at": "2026-07-17T10:30:00Z"
  },
  {
    "id": "asia",
    "name": "asia.pmtiles",
    "content_type": "map",
    "file_path": "/home/user/data/maps/asia.pmtiles",
    "file_size": 856588800,
    "checksum": null,
    "created_at": "2026-07-17T10:35:00Z"
  }
]
```

**Query Parameters**: (None yet; planned for v0.2: filter, search, sort)

**Use Case**: Populate map selector in UI.

---

### GET /api/content/books

List all available books.

**Request**:
```bash
curl http://localhost:8080/api/content/books
```

**Response** (200 OK):
```json
[
  {
    "id": "pride-prejudice",
    "name": "pride-and-prejudice.epub",
    "content_type": "book",
    "file_path": "/home/user/data/books/pride-and-prejudice.epub",
    "file_size": 524288,
    "checksum": null,
    "created_at": "2026-07-17T10:40:00Z"
  },
  {
    "id": "sherlock",
    "name": "complete-sherlock-holmes.pdf",
    "content_type": "book",
    "file_path": "/home/user/data/books/complete-sherlock-holmes.pdf",
    "file_size": 12582912,
    "checksum": null,
    "created_at": "2026-07-17T10:45:00Z"
  }
]
```

**Use Case**: Populate book browser in UI.

---

### GET /api/content/poi

List all available POI collections.

**Request**:
```bash
curl http://localhost:8080/api/content/poi
```

**Response** (200 OK):
```json
[
  {
    "id": "hospitals",
    "name": "hospitals.geojson",
    "content_type": "poi",
    "file_path": "/home/user/data/poi/hospitals.geojson",
    "file_size": 2097152,
    "checksum": null,
    "created_at": "2026-07-17T11:00:00Z"
  },
  {
    "id": "water-sources",
    "name": "water-sources.fgb",
    "content_type": "poi",
    "file_path": "/home/user/data/poi/water-sources.fgb",
    "file_size": 10485760,
    "checksum": null,
    "created_at": "2026-07-17T11:05:00Z"
  }
]
```

**Use Case**: Discover available POI datasets for map overlay.

---

## Download Management

### POST /api/download

Create a new download task.

**Request**:
```bash
curl -X POST http://localhost:8080/api/download \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/map-data.pmtiles"}'
```

**Request Body**:
```json
{
  "url": "https://example.com/map-data.pmtiles"
}
```

**Response** (201 Created):
```json
{
  "task_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Error Responses**:
- `400 Bad Request`: Invalid URL or missing field
  ```json
  { "error": "Invalid URL format" }
  ```
- `500 Internal Server Error`: Server issue
  ```json
  { "error": "Failed to create download task" }
  ```

**Use Case**: Start downloading a file from URL.

---

### GET /api/download/:task_id/status

Get download task status and progress.

**Request**:
```bash
curl http://localhost:8080/api/download/550e8400-e29b-41d4-a716-446655440000/status
```

**Response** (200 OK):
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "source": {
    "type": "url",
    "url": "https://example.com/map-data.pmtiles"
  },
  "status": "downloading",
  "progress": 0.65,
  "bytes_downloaded": 650000000,
  "total_bytes": 1000000000,
  "error": null,
  "content_type": "map",
  "created_at": "2026-07-17T10:30:00Z",
  "updated_at": "2026-07-17T10:35:00Z"
}
```

**Status Values**:
- `queued`: Waiting to start
- `downloading`: Actively downloading
- `validating`: Checking file format
- `routing`: Organizing file
- `completed`: Successfully ingested
- `failed`: Error occurred
- `cancelled`: User cancelled

**Error Response** (404 Not Found):
```json
{ "error": "Task not found" }
```

**Use Case**: Poll progress bar, show ETA, display errors.

---

### GET /api/downloads

List all download tasks (history and current).

**Request**:
```bash
curl http://localhost:8080/api/downloads
```

**Response** (200 OK):
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "source": { "type": "url", "url": "https://example.com/map-data.pmtiles" },
    "status": "completed",
    "progress": 1.0,
    "bytes_downloaded": 1000000000,
    "total_bytes": 1000000000,
    "error": null,
    "content_type": "map",
    "created_at": "2026-07-17T10:30:00Z",
    "updated_at": "2026-07-17T10:35:00Z"
  },
  {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "source": { "type": "url", "url": "https://example.com/novel.epub" },
    "status": "failed",
    "progress": 0.0,
    "bytes_downloaded": 0,
    "total_bytes": null,
    "error": "Connection timeout",
    "content_type": null,
    "created_at": "2026-07-17T10:40:00Z",
    "updated_at": "2026-07-17T10:42:00Z"
  }
]
```

**Query Parameters** (Planned v0.2):
- `status`: Filter by status (completed, failed, etc.)
- `limit`: Number of results (default: 100)
- `offset`: Pagination offset

**Use Case**: Display download history and queue in UI.

---

## File Serving (Planned v0.2)

### GET /maps/:id

Serve PMTiles map data with HTTP range requests.

**Request**:
```bash
curl -H "Range: bytes=0-1023" http://localhost:8080/maps/europe/tiles
```

**Response** (206 Partial Content):
- Binary PMTiles data

**Headers**:
```
Content-Type: application/octet-stream
Content-Length: 1024
Content-Range: bytes 0-1023/1000000000
Accept-Ranges: bytes
```

---

### GET /books/:id

Serve book file (EPUB, PDF, MOBI).

**Request**:
```bash
curl http://localhost:8080/books/pride-prejudice
```

**Response** (200 OK):
- Binary file data
- Browser downloads or opens in embedded reader

---

### GET /poi/:id

Serve POI data as GeoJSON.

**Request**:
```bash
curl http://localhost:8080/poi/hospitals
```

**Response** (200 OK):
```json
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "geometry": { "type": "Point", "coordinates": [2.3522, 48.8566] },
      "properties": { "name": "Hospital A", "beds": 120 }
    }
  ]
}
```

---

## Web Interface

### GET /

Serve main UI (SPA).

**Response** (200 OK):
- HTML page with Vue/React app
- Bundles JavaScript, CSS, assets
- App makes API calls to endpoints above

---

## Error Codes

| Code | Meaning | Example |
|------|---------|---------|
| 200 | Success | List operations |
| 201 | Created | POST /api/download |
| 206 | Partial Content | Range request for files |
| 400 | Bad Request | Invalid URL format |
| 404 | Not Found | Task ID doesn't exist |
| 500 | Server Error | Database failure |

---

## Request/Response Examples

### Example 1: Check Server Status

```bash
# Request
curl http://localhost:8080/api/status

# Response
{
  "version": "0.1.0",
  "status": "running",
  "data_dir": "/data",
  "content_count": {"maps": 3, "books": 12, "poi": 5}
}
```

### Example 2: Download a File

```bash
# Create download task
curl -X POST http://localhost:8080/api/download \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/osm-data.pmtiles"}'

# Response
{"task_id": "550e8400-e29b-41d4-a716-446655440000"}

# Check progress
curl http://localhost:8080/api/download/550e8400-e29b-41d4-a716-446655440000/status

# Response (after 30 seconds)
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "downloading",
  "progress": 0.35,
  "bytes_downloaded": 350000000,
  "total_bytes": 1000000000,
  ...
}

# Check again later
curl http://localhost:8080/api/download/550e8400-e29b-41d4-a716-446655440000/status

# Response (completed)
{
  "status": "completed",
  "progress": 1.0,
  "bytes_downloaded": 1000000000,
  ...
}
```

### Example 3: List Content

```bash
# Get available maps
curl http://localhost:8080/api/content/maps

# Response
[
  {"id": "europe", "name": "europe.pmtiles", "file_size": 1073741824},
  {"id": "usa", "name": "usa.pmtiles", "file_size": 856588800}
]
```

---

## Rate Limiting & Quotas (Planned v0.2)

- Download simultaneous limit: 10
- Upload size limit: 5 GB per file
- Task history retention: 1000 tasks

---

## CORS Policy

- Origin: `http://localhost:8080` (same-origin only)
- Methods: GET, POST
- Headers: Content-Type

No external domain access (security-first design).

---

**See Also**: [ARCHITECTURE.md](./ARCHITECTURE.md), [CONTENT_DOWNLOADER.md](./CONTENT_DOWNLOADER.md)
