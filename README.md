# Offline Nexus — Off-Grid Content Platform

Welcome to **Offline Nexus**, a self-contained, single-binary Rust application for offline content distribution and consumption in off-grid environments.

## Features

- **Single Static Binary**: Minimal dependencies, cross-platform (Linux, macOS, Windows, ARM)
- **Self-Contained Data Directory**: Organized storage for maps, books, and POIs
- **Built-in Content Downloader**: HTTP downloads and local file imports with automatic routing
- **Map Viewer**: Browse and interact with PMTiles map data
- **Book Reader**: Read EPUB books with basic navigation
- **POI Browser**: View point-of-interest data overlaid on maps
- **Lightweight**: ~15-20 MB binary, runs on Raspberry Pi and minimal hardware
- **No External Dependencies**: Data and UI bundled, no database required

## Quick Start

### Prerequisites
- Rust 1.70+ (for building) or download pre-compiled binary

### Installation & Run

**Option 1: From Source**
```bash
git clone https://github.com/yourusername/offline-nexus.git
cd offline-nexus
cargo build --release
./target/release/nexus
```

**Option 2: Pre-compiled Binary**
- Download from [Releases](https://github.com/yourusername/offline-nexus/releases)
- Run: `./nexus`

**Option 3: Docker**
```bash
docker run -p 8080:8080 -v data:/data offline-nexus:latest
```

### Access the UI
Open your browser: **http://localhost:8080**

## Data Directory Structure

```
data/
├── maps/              # PMTiles map files
│   └── regions/       # Optional subdirectories
├── books/             # EPUB, PDF, MOBI files
│   └── fiction/       # Optional subdirectories
├── poi/               # FlatGeoBuf, GeoJSON POI collections
│   └── custom/        # Optional subdirectories
└── inbox/             # Staging area for uploads
```

## Supported Content Formats

| Type | Formats | Storage |
|------|---------|---------|
| **Maps** | PMTiles | `/data/maps/` |
| **Books** | EPUB, PDF, MOBI | `/data/books/` |
| **POIs** | FlatGeoBuf (.fgb), GeoJSON | `/data/poi/` |

## Architecture Overview

- **Backend**: Axum web framework + Tokio async runtime
- **Frontend**: Browser-based UI (Vue/React) served from Rust backend
- **Content Ingestion**: Downloader module with file routing and validation
- **Deployment**: Single binary, Docker container, or pre-compiled images

See [ARCHITECTURE.md](./docs/ARCHITECTURE.md) for detailed system design.

## API Endpoints

### Status & Config
- `GET /api/status` — Server status and content inventory
- `GET /api/config` — Current configuration and directories

### Content Listing
- `GET /api/content/maps` — List available maps
- `GET /api/content/books` — List available books
- `GET /api/content/poi` — List available POI collections

### Download Management
- `POST /api/download` — Create new download task
- `GET /api/download/:task_id/status` — Check download progress
- `GET /api/downloads` — List all download tasks

See [API_REFERENCE.md](./docs/API_REFERENCE.md) for full documentation.

## Content Downloader

The built-in downloader supports:
- **HTTP/HTTPS URLs**: Download from remote servers
- **Local File Drop**: Import files via UI upload
- **Automatic Routing**: Files sorted by type (maps/, books/, poi/)
- **Validation**: Format checking with helpful warnings
- **Curated Defaults**: Pre-configured content sources (OSM, Gutenberg, etc.)

See [CONTENT_DOWNLOADER.md](./docs/CONTENT_DOWNLOADER.md) for detailed usage.

## Deployment Options

### Single Binary (Recommended)
```bash
# Build optimized release binary
cargo build --release
# Result: ~15-20 MB binary with no external dependencies
```

### Docker
```bash
docker build -t offline-nexus .
docker run -p 8080:8080 -v data:/data offline-nexus
```

### Raspberry Pi / ARM64
Pre-compiled ARM64 binaries available. Mount data volume for persistence:
```bash
./nexus-arm64
# Data automatically persists in ./data/
```

See [DEPLOYMENT.md](./docs/DEPLOYMENT.md) for detailed setup.

## Development

### Local Setup
```bash
# Install dependencies
rustup update

# Run in development mode
cargo run --bin nexus

# Run tests
cargo test

# Build with optimizations
cargo build --release
```

### Project Structure
```
offline-nexus/
├── crates/
│   ├── nexus-core/       # Shared types and configuration
│   ├── nexus-server/     # Axum API server
│   ├── nexus-downloader/ # Content ingestion
│   └── nexus-ui/         # Frontend build docs
├── docs/                 # Documentation
└── Dockerfile
```

See [DEV_SETUP.md](./docs/DEV_SETUP.md) for detailed developer guide.

## Roadmap

### v0.1.0 (Current)
- ✅ Basic content serving (maps, books, POIs)
- ✅ Built-in downloader with file routing
- ✅ Browser-based UI
- ✅ Single binary deployment

### v0.2.0 (Planned)
- 📋 Full-text search (books, POIs)
- 📋 Advanced map controls (offline vector tiles)
- 📋 Book annotations and bookmarks
- 📋 Content sync and backup

### v1.0.0 (Future)
- 📋 Radio support (RTL-SDR integration)
- 📋 Peer-to-peer content sync
- 📋 Multi-user support (authentication)
- 📋 Content transcoding and format conversion

See [RADIO_PLANNING.md](./docs/RADIO_PLANNING.md) for radio roadmap (v2+).

## Configuration

Configuration is auto-created on first run:
- **Data Directory**: `./data/` (or set `DATA_DIR` environment variable)
- **Server Host**: `127.0.0.1` (default, configurable)
- **Server Port**: `8080` (default, configurable)

## Troubleshooting

### Binary won't start
- Ensure port 8080 is available: `lsof -i :8080`
- Check data directory permissions: `ls -la ./data/`
- Review logs for errors: `RUST_LOG=debug ./nexus`

### Content not appearing
- Verify files are in correct subdirectory (`maps/`, `books/`, `poi/`)
- Check file format is supported (see table above)
- Review download status in UI

### Performance issues
- Reduce number of files in single directory (use subdirectories)
- Ensure sufficient disk space for data
- Check system resources: `free -h`, `df -h`

## Contributing

Contributions welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Submit a pull request

See [Contributing Guide](./CONTRIBUTING.md) for details.

## License

Dual-licensed under MIT or Apache 2.0. See [LICENSE](./LICENSE) for details.

## Support

- **Documentation**: See [docs/](./docs/)
- **Issues**: GitHub Issues
- **Discussions**: GitHub Discussions

---

**Built with ❤️ for offline resilience and connectivity.**
