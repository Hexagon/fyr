# ARCHITECTURE.md — Offline Nexus System Design

## System Overview

Offline Nexus is a self-contained, distributed content platform designed for offline and off-grid scenarios. It uses a simple client-server architecture with a single binary backend and browser-based UI.

```
┌─────────────────────────────────────────────────────────────┐
│                    Web Browser (Any Device)                 │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Vue/React SPA (Dashboard, Downloader, Viewer)       │  │
│  └──────────────────┬──────────────────────────────────┘  │
└─────────────────────┼──────────────────────────────────────┘
                      │ HTTP (localhost:8080)
┌─────────────────────┼──────────────────────────────────────┐
│  Nexus Server (nexus-server)                              │
│  ┌─────────────────────────────────────────────────────┐  │
│  │ Axum Web Server (tokio async runtime)              │  │
│  │  • API Router (/api/*)                             │  │
│  │  • Static File Serving (UI assets)                 │  │
│  │  • Request/Response Handlers                       │  │
│  └────────┬──────────────────────────────┬────────────┘  │
└───────────┼──────────────────────────────┼────────────────┘
            │                              │
    ┌───────▼──────────┐      ┌────────────▼──────────┐
    │  Content Manager │      │  Download Manager    │
    │ (nexus-core)     │      │ (nexus-downloader)   │
    │                  │      │                      │
    │ • Content Type   │      │ • Task Queue         │
    │ • Metadata       │      │ • Download Engine    │
    │ • Validation     │      │ • File Router        │
    │ • Config         │      │ • Progress Tracking  │
    └────────┬─────────┘      └────────┬─────────────┘
             │                         │
    ┌────────▼─────────────────────────▼────────┐
    │         Data Directory (./data/)          │
    │                                           │
    │  maps/          books/       poi/    inbox/
    │  ├── regions/   ├── fiction/  ├── etc/
    │  └── ...        └── ...       └── ...
    │                                           │
    │  • PMTiles files    • EPUB/PDF           │
    │  • Map metadata     • MOBI               │
    │                     • Book metadata      │
    │
    │                • FlatGeoBuf (.fgb)      │
    │                • GeoJSON                │
    │                • POI collections        │
    └───────────────────────────────────────────┘
```

## Module Responsibilities

### 1. nexus-core (Content Management)
**Location**: `crates/nexus-core/`

Provides shared types and configuration:
- **ContentType** enum: Maps, Books, POIs
- **DownloadTask**: Status, progress, errors
- **ContentMetadata**: File information, creation timestamp
- **Config**: Data directory structure, server configuration
- **ValidationResult**: File validation (warn-only strategy)

**Key Behavior**:
- No I/O; pure data structures
- File extension mapping to content types
- Directory structure initialization

**Dependencies**: serde, uuid, tokio (async types)

### 2. nexus-server (API & UI)
**Location**: `crates/nexus-server/`

Axum-based web server serving:
- JSON API endpoints (`/api/*`)
- Static UI assets (SPA frontend)
- Request handlers for content listing, downloads, status

**Routes**:
```
GET  /                          → Serve SPA (index.html)
GET  /api/status                → Server status + inventory
GET  /api/config                → Configuration info
GET  /api/content/maps          → List map files
GET  /api/content/books         → List book files
GET  /api/content/poi           → List POI collections
POST /api/download              → Create download task
GET  /api/download/:task_id     → Download progress
GET  /api/downloads             → List all tasks
```

**State Management**:
- `AppState` struct holds Config and DownloadManager
- Shared via Axum State extension

### 3. nexus-downloader (Content Ingestion)
**Location**: `crates/nexus-downloader/`

Handles content acquisition and organization:
- **DownloadEngine**: HTTP/HTTPS downloads with resume support (TODO)
- **ContentRouter**: Routes files to maps/, books/, poi/ based on extension
- **DownloadManager**: Task queue (in-memory, with future persistence)

**File Routing Logic**:
```
Extension → ContentType → Directory
.pmtiles  → Map        → data/maps/
.epub     → Book       → data/books/
.pdf      → Book       → data/books/
.mobi     → Book       → data/books/
.fgb      → Poi        → data/poi/
.geojson  → Poi        → data/poi/
```

**Task Lifecycle**:
1. User submits URL or file
2. Task created with status=Queued
3. Engine downloads/imports file
4. Router validates extension, determines destination
5. File moved to appropriate directory
6. Task status updated (Completed or Failed)

### 4. nexus-ui (Frontend)
**Location**: `crates/nexus-ui/` + `static/` (future)

Browser-based SPA with components:
- **Dashboard**: Content inventory (maps, books, POIs)
- **Content Downloader**: URL input, file upload, task queue
- **Map Viewer**: Leaflet/OpenLayers + PMTiles
- **Book Reader**: EPUB reader
- **POI Browser**: GeoJSON table view

**Current State**: Stub (placeholder HTML)
**Implementation**: Vue 3 or React (TBD)

### 5. nexus-radio (Planning, v2+)
**Location**: `crates/nexus-radio/` (not yet created)

RTL-SDR integration (stubs only):
- Frequency tuning API
- Signal strength reporting
- Audio output routing
- Recording capability (future)

See [RADIO_PLANNING.md](./RADIO_PLANNING.md) for roadmap.

---

## Data Flow Diagrams

### Content Discovery Flow

```
User opens http://localhost:8080
         ↓
Browser requests GET /
         ↓
nexus-server serves index.html (SPA)
         ↓
Vue/React app initializes
         ↓
App requests GET /api/content/maps
         ↓
nexus-server:
  1. Reads config.maps_dir()
  2. Walks directory with walkdir
  3. Collects metadata (filename, size, etc.)
  4. Returns JSON
         ↓
Browser displays map list
```

### Download Flow

```
User enters URL or uploads file
         ↓
Browser POST /api/download { url: "..." }
         ↓
nexus-server:handler:create_download()
  1. Create DownloadTask (Queued status)
  2. Return task_id to client
         ↓
Browser polls GET /api/download/{task_id}/status
         ↓
Backend (future):
  1. DownloadEngine fetches file
  2. Updates progress (% complete)
  3. ContentRouter determines destination
  4. Validates file (warn-only)
  5. Moves file to data/maps/ (or books/ or poi/)
  6. Updates task status (Completed)
         ↓
Browser shows "Download Complete"
```

### File Routing Flow

```
Downloaded file: "europe.pmtiles"
         ↓
ContentRouter:
  1. Extract extension: ".pmtiles"
  2. ContentType::from_extension("pmtiles") → Some(Map)
  3. Determine destination: data/maps/
  4. Final path: data/maps/europe.pmtiles
         ↓
File moved to destination
```

---

## Configuration & Startup

**Default Startup**:
1. Load `Config` (default: data_dir = "./data")
2. Call `Config::initialize_directories()` → creates maps/, books/, poi/, inbox/
3. Start Axum server on 127.0.0.1:8080
4. Serve UI on GET /

**Customization** (future):
- Config file: `nexus.toml` or environment variables
- Custom data directory: `DATA_DIR=/mnt/usb/data ./nexus`
- Custom port: `PORT=8081 ./nexus`

---

## Content Type Support

### Maps (PMTiles)
- **Format**: PMTiles 3.0+
- **Storage**: `data/maps/`
- **Serving**: HTTP range requests for efficient tile loading
- **Viewer**: Leaflet/OpenLayers with @maplibre/pmtiles
- **Validation**: Header check (warn if not "PMTiles")

### Books (EPUB/PDF/MOBI)
- **Formats**: EPUB (preferred), PDF, MOBI
- **Storage**: `data/books/`
- **Serving**: File download + embedded reader
- **Reader**: EPUB.js or Readium (lightweight)
- **Validation**: ZIP structure check for EPUB

### POIs (FlatGeoBuf/GeoJSON)
- **Formats**: FlatGeoBuf (.fgb), GeoJSON
- **Storage**: `data/poi/`
- **Serving**: GeoJSON conversion (if needed)
- **Overlay**: Map viewer displays POIs
- **Validation**: Basic JSON/FGB structure check

---

## Validation Strategy (Warn-Only)

**Design Principle**: Trust user files, warn gracefully, fail safe

1. **File Extension**: Detected automatically
2. **Format Validation**: Attempt to read header/structure
3. **On Invalid Format**:
   - Log warning
   - Add warning to ValidationResult
   - **Still ingest file** (user can decide)
   - Mark as potentially invalid in UI

**Benefits**:
- User flexibility (can import partial/corrupted files)
- Fail-graceful behavior (don't stop pipeline on warnings)
- Helpful feedback (warnings guide user)

---

## Performance Considerations

### Single Binary Optimization
- **LTO**: Link-time optimization (profile.release: lto = "fat")
- **Strip Symbols**: Binary stripped of debug info
- **Target Size**: ~15-20 MB

### Data Directory Scalability
- **Subdirectories**: Supported (data/maps/regions/, data/books/fiction/)
- **File Limit**: No hard limit, but recommend < 1000 files per directory
- **Disk I/O**: Minimized (content discovery on startup, cached in UI)

### Memory Usage
- **Task Queue**: In-memory (scales with active downloads, ~1 MB per 1000 tasks)
- **File Listing**: Cached on first load, updated on request

---

## Security & Isolation

### Data Isolation
- All content in single data directory (easy backup/reset)
- No database (no SQL injection risk)
- File validation (sanitize paths, prevent directory traversal)

### Network Isolation
- Server binds to 127.0.0.1 (localhost only, no external exposure)
- Future: Optional password authentication (v0.2+)

### File Safety
- Downloaded files validated (warn-only, no execution)
- No automatic script execution
- CORS policy: Same-origin only (localhost)

---

## Error Handling & Resilience

### Download Failures
- Task marked as Failed
- Error message stored in DownloadTask
- UI displays error to user
- File cleaned up or marked as incomplete

### Corrupted Content
- Validation detects issues
- Warning logged
- File still accessible (user can try opening)
- UI shows warning badge

### Server Restart
- Data directory persists
- In-memory task queue lost (TODO: add persistence)
- Content re-discovered on startup

---

## Future Enhancements (v0.2+)

1. **Task Persistence**: Save download tasks to disk for recovery
2. **Caching Layer**: Cache tile requests, book chapters
3. **Full-Text Search**: Index books and POI metadata
4. **Content Versioning**: Track versions of maps, books
5. **P2P Sync**: Share content between Nexus instances
6. **Radio Support**: RTL-SDR frequency tuning and recording

---

## Deployment Targets

### Single Binary
- Linux x86_64, ARM64
- macOS x86_64, ARM64
- Windows x86_64

### Docker
- Multi-stage build
- ~100 MB image size
- Volume mount for data persistence

### Embedded Devices
- Raspberry Pi (ARM64)
- Jetson Nano
- Any Linux system with 64-bit CPU

---

**See Also**: [API_REFERENCE.md](./API_REFERENCE.md), [DATA_FORMATS.md](./DATA_FORMATS.md), [CONTENT_DOWNLOADER.md](./CONTENT_DOWNLOADER.md)
