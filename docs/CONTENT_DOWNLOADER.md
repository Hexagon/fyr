# CONTENT_DOWNLOADER.md — Download Engine & File Routing

## Overview

The Offline Nexus content downloader is the central hub for acquiring and organizing content. It supports:
- HTTP/HTTPS downloads with progress tracking
- Local file uploads via web interface
- Automatic file routing to appropriate directories
- Content validation with helpful warnings
- Download task queue and history

## Architecture

```
User Input
├── Enter URL
└── Upload File
    ↓
Download Manager (Task Queue)
    ↓
Download Engine (HTTP/File I/O)
    ↓
Content Validation
    ↓
Content Router (File Organization)
    ↓
Data Directory (maps/, books/, poi/)
```

## Task Lifecycle

### State Machine

```
┌────────────────────────────────────────────────────┐
│                   Download Task                    │
│  id, source, status, progress, error, content_type │
└────────────────────────────────────────────────────┘

User Submits URL/File
        ↓
[Queued] ← Initial state, awaiting processing
        ↓
[Downloading] ← Fetching file from source
        ↓
[Validating] ← Checking file format and structure
        ↓
[Routing] ← Determining destination directory
        ↓
[Completed] ← Success, file ingested and organized
        ├─ File moved to maps/, books/, or poi/
        ├─ Metadata extracted
        └─ Task retained for history

Alternative: [Failed] ← Error during any stage
        ├─ Error message logged
        ├─ File marked as incomplete
        └─ User can retry or investigate
```

### Task Status Values

```rust
pub enum DownloadStatus {
    Queued,      // Waiting to download
    Downloading, // Fetching from source
    Validating,  // Format/structure checking
    Routing,     // Determining destination
    Completed,   // Successfully ingested
    Failed,      // Error occurred
    Cancelled,   // User cancelled
}
```

## API Endpoints

### Create Download Task
**Endpoint**: `POST /api/download`

**Request**:
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

### Get Task Status
**Endpoint**: `GET /api/download/:task_id/status`

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

### List All Tasks
**Endpoint**: `GET /api/downloads`

**Response** (200 OK):
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "source": { "type": "url", "url": "..." },
    "status": "completed",
    ...
  },
  {
    "id": "660e8400-e29b-41d4-a716-446655440001",
    "source": { "type": "url", "url": "..." },
    "status": "failed",
    "error": "Connection timeout",
    ...
  }
]
```

## Download Engine

### HTTP Downloads

**Implementation**: `nexus-downloader/src/download.rs:DownloadEngine`

**Features**:
- Async I/O with Tokio
- Progress callback (bytes_downloaded / total_bytes)
- Resume capability (HTTP Range requests) — TODO v0.2
- Timeout handling
- Redirect following (up to 5 hops)
- Error recovery

**Example Flow**:
```rust
let engine = DownloadEngine::start_download("https://example.com/file.pmtiles").await?;
loop {
    let progress = engine.progress().await?;
    println!("Downloaded {:.1}%", progress * 100.0);
    if progress >= 1.0 { break; }
    tokio::time::sleep(Duration::from_millis(500)).await;
}
```

### Local File Uploads

**Implementation**: `nexus-server/src/handlers.rs` (TODO)

**Features**:
- Drag-and-drop in browser UI
- Form-based file upload
- Progress bar in UI
- Size limits (configurable, default 5 GB)

**Flow**:
1. User selects file in browser
2. Browser sends multipart/form-data to `/api/upload`
3. Server saves to inbox/ directory
4. Router processes file (same as downloaded files)
5. File moved to final destination

## Content Router

### Routing Logic

**Implementation**: `nexus-downloader/src/router.rs:ContentRouter`

**Algorithm**:
```
Input: file_path, data_dir
  1. Extract file extension
  2. ContentType::from_extension(ext) → Option<ContentType>
  3. If unknown → log warning, skip routing
  4. Get destination directory: data_dir / ContentType.directory_name()
  5. Preserve subdirectories if present
  6. Return final_path
  7. Move file to final_path
```

**Example Routing**:
```
Input:  inbox/2026-07-17/europe.pmtiles
Output: data/maps/europe.pmtiles

Input:  inbox/fiction_books/novel.epub
Output: data/books/novel.epub

Input:  uploads/pois/hospitals.geojson
Output: data/poi/hospitals.geojson
```

### Subdirectory Support

Files can be organized in subdirectories within each content type:

```
Input:  inbox/detailed/london-streets.pmtiles
Output: data/maps/detailed/london-streets.pmtiles
        ↑─────────────────────────── Subdirectory preserved

Input:  inbox/scifi/foundation.epub
Output: data/books/scifi/foundation.epub
```

**Discovery**: Subdirectories are scanned recursively; all files discovered automatically.

## Content Validation

### Validation Flow

See `nexus-core/src/validation.rs`

```
File to Ingest
    ↓
detect_extension()
    ├─ Known extension → Identified type
    ├─ Unknown → Log warning "Unknown extension"
    └─ No extension → Skip
    ↓
validate_file(content_type)
    ├─ Read file header/structure
    ├─ Check format-specific rules
    ├─ Collect warnings
    └─ Return ValidationResult { valid, warnings, errors }
    ↓
Decision
    ├─ valid=true, no warnings → "Clean" badge
    ├─ valid=true, warnings → "Warning" badge
    └─ valid=false → "Invalid" badge (but still ingest)
    ↓
Ingest File (Regardless of Result)
```

### Validation Rules per Format

| Format | Checks | Fails If | Continues? |
|--------|--------|----------|-----------|
| PMTiles | Header "PMTiles" | Not found | ✅ Yes |
| EPUB | ZIP structure, OPF | Invalid ZIP | ✅ Yes |
| PDF | Header "%PDF" | Not found | ✅ Yes |
| FGB | FGB header, feature count | Invalid | ✅ Yes |
| GeoJSON | JSON parse, FeatureCollection | Invalid JSON | ✅ Yes |

**Principle**: All checks are warnings; files ingest regardless.

## Curated Content Sources

### Pre-configured Downloads

A set of curated, vetted content sources for one-click adding:

**Maps**:
- OpenStreetMap (various regions)
- Natural Earth (reference maps)
- GEBCO (bathymetry)

**Books**:
- Project Gutenberg (classic literature)
- Standard Ebooks (curated public domain)
- Open Library (millions of titles)

**POIs**:
- OSM POIs (filtered by category)
- Wikidata locations
- Humanitarian Data Exchange

### UI Integration

```
┌─────────────────────────────────┐
│ Downloader Interface            │
├─────────────────────────────────┤
│ URL Input: [_____________]      │
│ [Download] [Clear]              │
│                                 │
│ Curated Sources:                │
│ ├─ [Download OSM Europe]        │
│ ├─ [Download Gutenberg Books]   │
│ └─ [More Sources...]            │
│                                 │
│ Active Downloads:               │
│ ├─ map-data.pmtiles: 65%        │
│ └─ novel.epub: 100% ✓           │
└─────────────────────────────────┘
```

### Configuration

Sources defined in `config/content-sources.toml` (future):
```toml
[[source]]
name = "OpenStreetMap Europe"
url = "https://osm.example.com/europe.pmtiles"
type = "map"
description = "High-detail European map tiles"

[[source]]
name = "Project Gutenberg"
url = "https://gutenberg.org/cache/epub/top_100.zip"
type = "book"
description = "Top 100 classic books"
```

## Error Handling

### Common Errors

| Error | Cause | Recovery |
|-------|-------|----------|
| Connection timeout | Network unreachable | Retry (exponential backoff) |
| HTTP 404 | URL not found | User checks URL |
| File corrupted | Interrupted download | Retry or resume |
| Disk full | No space on device | User frees space |
| Invalid format | File not recognized | Warn, continue ingesting |
| Permission denied | Directory not writable | Check data directory permissions |

### Error Display in UI

```
Download Failed: map-data.pmtiles

Error: Connection timeout after 30s
Status: Failed

[Retry] [Delete Task] [Report Issue]
```

### Task Error Field

```json
{
  "error": "Connection timeout: read timed out after 30s",
  "status": "failed",
  ...
}
```

## Task Persistence (v0.2)

**Current** (v0.1): In-memory task queue (lost on server restart)

**Planned** (v0.2): Persist tasks to disk:
```
data/
└── .nexus/
    ├── tasks.db          # SQLite task history
    ├── in-progress/      # Unfinished downloads
    └── metadata.json
```

## Performance & Scalability

### Concurrent Downloads
- Tokio allows 100+ concurrent downloads
- Per-task memory: ~1 MB
- I/O limited by disk/network

### File Size Handling
- Streaming download (not loaded fully in memory)
- Progress updated every 1-5 seconds
- Large files (1-5 GB) supported

### Optimization Tips
- Use PMTiles for maps (efficient tiling)
- Compress books (EPUB smaller than PDF)
- Pre-compute POI collections (avoid on-the-fly processing)

## Development Notes

### Testing

**Unit Tests**:
```bash
cargo test -p nexus-downloader
# Tests ContentRouter, file routing logic
```

**Integration Tests** (TODO):
```bash
# Test full download flow end-to-end
# Test error recovery
# Test concurrent downloads
```

### Future Enhancements
- [ ] Resume downloads (HTTP Range)
- [ ] Bandwidth throttling
- [ ] Scheduled downloads (e.g., 2 AM)
- [ ] Mirror support (fallback URLs)
- [ ] Content deduplication
- [ ] Peer-to-peer sync

---

**See Also**: [ARCHITECTURE.md](./ARCHITECTURE.md), [API_REFERENCE.md](./API_REFERENCE.md), [DATA_FORMATS.md](./DATA_FORMATS.md)
