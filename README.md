# Offline Nexus
## Offline Wikipedia • Survival Guides • Maps & POIs for Off-Grid Communities

> A self-contained, single-binary platform for offline access to Wikipedia, survival manuals, emergency medical guides, mechanical handbooks, and detailed offline maps—deployable anywhere, no internet required.

<!-- TODO: Add logo here (200x200px SVG or PNG) -->

---

## Perfect For

- 🚨 **Disaster Response**: Access medical protocols, emergency procedures, building safely when networks fail
- 🌍 **Remote Communities**: Villages, research stations, maritime vessels without reliable connectivity
- 🧭 **Field Research**: Expeditions, NGO operations, development projects with offline data access
- ⚙️ **Mechanical Repair**: Service technicians, farmers, maintenance workers needing technical references
- 🏥 **Healthcare Workers**: Nurses, midwives, field clinics with limited internet
- 📚 **Education**: Schools, libraries, homeschooling in off-grid areas
- 🛡️ **Preparedness**: Emergency supplies, survival information, resilience planning

---

## What You Get

- **Offline Wikipedia**: Full, simplified, or language-specific versions—searchable, no internet needed
- **Survival & Reference Books**: Medical handbooks, agricultural guides, mechanical manuals, emergency procedures
- **Interactive Offline Maps**: OpenStreetMap, satellite imagery, custom POI overlays (hospitals, water sources, etc.)
- **Single 1.54 MB Binary**: No complex setup, runs on Linux, macOS, Windows, Raspberry Pi, any modern device
- **Zero External Dependencies**: Works fully offline—no cloud, no database, no external services
- **Browser-Based UI**: Works on any device with a web browser (phone, tablet, laptop)

---

## Quick Start

### Prerequisites
- Pre-compiled binary **OR** Rust 1.70+ (for building)

### Installation & Run

**1️⃣ Download Binary (Recommended)**
```bash
# macOS/Linux
curl -Lo nexus https://github.com/Hexagon/offline-nexus/releases/download/v0.1.0/nexus-x86_64-linux
chmod +x nexus

# Raspberry Pi (ARM64)
curl -Lo nexus https://github.com/Hexagon/offline-nexus/releases/download/v0.1.0/nexus-aarch64-linux
chmod +x nexus

# Run
./nexus
# → Access at http://localhost:8080
```

**2️⃣ Build from Source**
```bash
git clone https://github.com/Hexagon/offline-nexus.git
cd offline-nexus
cargo build --release
./target/release/nexus
```

**3️⃣ Docker**
```bash
docker run -p 8080:8080 -v data:/data \
  ghcr.io/Hexagon/offline-nexus:latest
```

---

## 📚 Adding Content

### Wikipedia (Easiest Start)

Download offline Wikipedia snapshots from [Kiwix](https://kiwix.org):

```bash
# Get simplified Wikipedia (2 GB, sufficient for basics)
wget https://library.kiwix.org/download/wikipedia_en_simple.zim

# Place in data/books/
mkdir -p data/books
mv wikipedia_en_simple.zim data/books/

# UI automatically detects it
```

**See**: [WIKIPEDIA_SUPPORT.md](./docs/WIKIPEDIA_SUPPORT.md) for curated presets (medical, survival, engineering, education)

### Offline Maps

Add OpenStreetMap or satellite imagery:

```bash
# Download optimized map (Europe, 2 GB)
wget https://maps.protomaps.com/downloads/pmtiles/2024_01_v3_europe.pmtiles
mv 2024_01_v3_europe.pmtiles data/maps/
```

**See**: [PMTILES_GENERATION.md](./docs/PMTILES_GENERATION.md) for custom map creation

### Books & Guides

Copy EPUB, PDF, or text files:

```bash
cp survival-guide.epub data/books/
cp medical-handbook.epub data/books/
# Restart, and they appear in UI
```

---

## 🏥 Use Case Example: Rural Clinic

```bash
# 1. Start with Offline Nexus
./nexus

# 2. Add medical Wikipedia
# → Download from Kiwix (medical subset, 3 GB)
# → Place in data/books/

# 3. Add emergency guides
# → EPUB files: emergency protocols, drug database
# → Place in data/books/

# 4. Share on local network
# → Doctor opens http://clinic-server:8080
# → Access protocols, drug interactions, diagnoses

# Zero internet needed ✓
# Works on older devices ✓
# Accessible to all staff ✓
```

---

## 📋 Content Types

| Type | What Goes Here | Example |
|------|---|---|
| **Maps** | PMTiles, offline map files | OpenStreetMap, satellite imagery, hiking maps |
| **Books** | EPUB, PDF, MOBI, ZIM | Wikipedia, medical handbooks, technical guides, survival manuals |
| **POIs** | GeoJSON, FlatGeoBuf | Hospital locations, water sources, repair shops, research stations |

---

## 🚀 Deployment Options

| Target | Command | Notes |
|--------|---------|-------|
| **Local Dev** | `./nexus` | Listens on http://127.0.0.1:8080 |
| **Network** | `NEXUS_HOST=0.0.0.0 ./nexus` | Accessible from other devices on network |
| **Raspberry Pi** | See [ARM deployment](./docs/DEVELOPER_DEPLOYMENT.md#deployment-raspberry-pi-arm64) | Battery-powered, silent, reliable |
| **Docker** | `docker-compose up` | Easier scaling, isolation |
| **Systemd** | See [Linux deployment](./docs/DEVELOPER_DEPLOYMENT.md#deployment-linux-server-systemd) | Production server, auto-restart |

---

### Quick Data Setup

### Quick Data Setup

```bash
# Binary auto-creates ./data/ with structure:
# data/
# ├── maps/       # PMTiles, map files
# ├── books/      # EPUB, PDF, Wikipedia ZIM files
# ├── poi/        # GeoJSON, FlatGeoBuf files
# └── inbox/      # Upload staging

# Just drop files in directories and restart:
cp my-map.pmtiles data/maps/
cp my-book.epub data/books/
./nexus  # Restarts and detects new content
```

---

## 📖 Documentation

### For End Users
- [WIKIPEDIA_SUPPORT.md](./docs/WIKIPEDIA_SUPPORT.md) — Wikipedia setup, curated presets (medical, survival, engineering)
- [PMTILES_GENERATION.md](./docs/PMTILES_GENERATION.md) — Create custom offline maps
- [DEPLOYMENT.md](./docs/DEPLOYMENT.md) — Run on various hardware

### For Developers
- [DEVELOPER_DEPLOYMENT.md](./docs/DEVELOPER_DEPLOYMENT.md) — Systemd, Docker, Raspberry Pi, monitoring
- [DEV_SETUP.md](./docs/DEV_SETUP.md) — Development environment, testing, debugging
- [ARCHITECTURE.md](./docs/ARCHITECTURE.md) — System design, modules, data flow
- [API_REFERENCE.md](./docs/API_REFERENCE.md) — HTTP API endpoints, examples

---

## 💻 Under the Hood

- **Language**: Rust (fast, memory-safe, minimal dependencies)
- **Binary Size**: 1.54 MB (optimized, includes full server + API)
- **Framework**: Axum web framework + Tokio async runtime
- **Database**: None—files are the database
- **Frontend**: Browser-based UI (Vue/React, roadmap)
- **Deployment**: Single static binary, Docker, or systemd service

---

### Quick Data Setup

```bash
# Binary auto-creates ./data/ with structure:
# data/
# ├── maps/       # PMTiles, map files
# ├── books/      # EPUB, PDF, Wikipedia ZIM files
# ├── poi/        # GeoJSON, FlatGeoBuf files
# └── inbox/      # Upload staging

# Just drop files in directories and restart:
cp my-map.pmtiles data/maps/
cp my-book.epub data/books/
./nexus  # Restarts and detects new content
```

---

## 📖 Documentation

### For End Users
- [WIKIPEDIA_SUPPORT.md](./docs/WIKIPEDIA_SUPPORT.md) — Wikipedia setup, curated presets (medical, survival, engineering)
- [PMTILES_GENERATION.md](./docs/PMTILES_GENERATION.md) — Create custom offline maps
- [DEPLOYMENT.md](./docs/DEPLOYMENT.md) — Run on various hardware

### For Developers
- [DEVELOPER_DEPLOYMENT.md](./docs/DEVELOPER_DEPLOYMENT.md) — Systemd, Docker, Raspberry Pi, monitoring
- [DEV_SETUP.md](./docs/DEV_SETUP.md) — Development environment, testing, debugging
- [ARCHITECTURE.md](./docs/ARCHITECTURE.md) — System design, modules, data flow
- [API_REFERENCE.md](./docs/API_REFERENCE.md) — HTTP API endpoints, examples

---

## 💻 Under the Hood

- **Language**: Rust (fast, memory-safe, minimal dependencies)
- **Binary Size**: 1.54 MB (optimized, includes full server + API)
- **Framework**: Axum web framework + Tokio async runtime
- **Database**: None—files are the database
- **Frontend**: Browser-based UI (Vue/React, roadmap)
- **Deployment**: Single static binary, Docker, or systemd service

---

## 📂 Project Structure

```
offline-nexus/
├── crates/
│   ├── types/        # Shared types, config, validation
│   ├── server/       # Axum API server + router
│   ├── downloader/   # Content download engine, file routing
│   └── ui/           # Frontend build coordination
├── docs/
│   ├── WIKIPEDIA_SUPPORT.md
│   ├── PMTILES_GENERATION.md
│   ├── DEVELOPER_DEPLOYMENT.md
│   ├── ARCHITECTURE.md
│   ├── API_REFERENCE.md
│   ├── DEV_SETUP.md
│   └── DATA_FORMATS.md
├── Cargo.toml        # Workspace manifest
├── Dockerfile        # Multi-stage build
└── README.md         # This file
```

---

## 🛣️ Roadmap

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
