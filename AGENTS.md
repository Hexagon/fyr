# AGENTS.md — Offline Nexus Development Guide

This document serves as the central hub for coordinating development, documentation, and code organization. Use this file to understand module responsibilities, keep docs synchronized with code, and delegate tasks to development agents.

---

## Module Overview & Maintenance

Each crate is maintained by specific agent tasks. Documentation must stay synchronized with code.

### Core Module: `nexus-core`
**Path**: `crates/nexus-core/src/`

**Responsibility**: Shared types, configuration, and validation logic

**Files**:
- `lib.rs` — Module exports
- `types.rs` — Core data structures (ContentType, DownloadTask, Metadata)
- `config.rs` — Configuration management and directory initialization
- `validation.rs` — File format validation (warn-only strategy)

**Documentation**: [DATA_FORMATS.md](./docs/DATA_FORMATS.md)

**Agent Task**: `@core-agent: Review ContentType enum and file validation logic to ensure new formats are added consistently`

**Key Types**:
- `ContentType` enum (Map, Book, Poi)
- `DownloadTask` struct with status tracking
- `Config` struct for data directory management
- `ValidationResult` with warnings/errors

---

### Server Module: `nexus-server`
**Path**: `crates/nexus-server/src/`

**Responsibility**: Axum API endpoints, request handlers, static file serving

**Files**:
- `main.rs` — Application entry point and router setup
- `handlers.rs` — HTTP handlers for all API endpoints
- `state.rs` — Shared application state (AppState)

**Documentation**: [API_REFERENCE.md](./docs/API_REFERENCE.md)

**Agent Task**: `@server-agent: Ensure all endpoints in handlers.rs are documented in API_REFERENCE.md with request/response examples`

**Endpoints**:
| Method | Path | Handler | Status |
|--------|------|---------|--------|
| GET | `/api/status` | `status()` | ✅ Impl |
| GET | `/api/config` | `config()` | ✅ Impl |
| GET | `/api/content/maps` | `list_maps()` | ✅ Impl |
| GET | `/api/content/books` | `list_books()` | ✅ Impl |
| GET | `/api/content/poi` | `list_poi()` | ✅ Impl |
| POST | `/api/download` | `create_download()` | ✅ Impl |
| GET | `/api/download/:task_id/status` | `get_download_status()` | ✅ Impl |
| GET | `/api/downloads` | `list_downloads()` | ✅ Impl |
| GET | `/` | `serve_ui()` | ⚠️ Stub (placeholder HTML) |

---

### Downloader Module: `nexus-downloader`
**Path**: `crates/nexus-downloader/src/`

**Responsibility**: Download engine, file routing, and download task management

**Files**:
- `lib.rs` — Module exports
- `download.rs` — HTTP download engine (DownloadEngine, Download)
- `router.rs` — File routing logic (ContentRouter)
- `manager.rs` — Download task queue (DownloadManager)

**Documentation**: [CONTENT_DOWNLOADER.md](./docs/CONTENT_DOWNLOADER.md)

**Agent Task**: `@downloader-agent: Implement HTTP download engine with progress tracking and ensure router tests pass`

**Current Status**: 
- ✅ ContentRouter with file routing logic
- ✅ Router tests (pmtiles, epub, fgb routing)
- ⚠️ DownloadEngine (stub, needs implementation)
- ⚠️ Progress tracking (TODO)
- ⚠️ Resume capability (TODO for v0.2)

---

### UI Module: `nexus-ui`
**Path**: `crates/nexus-ui/` + `static/` (future)

**Responsibility**: Frontend build coordination, Vue/React SPA

**Current Status**: ⚠️ Stub (documentation only)

**Agent Task**: `@ui-agent: Set up Vue/React build pipeline, create dashboard, downloader UI, map viewer, book reader, POI browser`

**UI Components** (TODO):
- Dashboard — Content inventory display
- Downloader — URL input, file upload, task queue
- Map Viewer — Leaflet/OpenLayers with PMTiles
- Book Reader — EPUB.js or similar
- POI Browser — GeoJSON table/map view

**Documentation**: [UI_DEVELOPMENT.md](./docs/UI_DEVELOPMENT.md) (not yet created)

---

### Radio Module: `nexus-radio` (v2+ Planning)
**Path**: `crates/nexus-radio/src/` (not yet created)

**Responsibility**: RTL-SDR integration planning (stubs only)

**Current Status**: 📋 Planning only

**Documentation**: [RADIO_PLANNING.md](./docs/RADIO_PLANNING.md)

**Agent Task**: `@radio-agent: Research RTL-SDR Rust libraries and design API for frequency tuning, signal strength, and recording`

---

## Documentation Index

All documentation lives in `docs/` directory. Agents must keep docs synchronized with code.

| Document | Purpose | Maintains | Last Updated |
|----------|---------|-----------|--------------|
| [ARCHITECTURE.md](./docs/ARCHITECTURE.md) | System design, data flow, module interaction | @core-agent | — |
| [API_REFERENCE.md](./docs/API_REFERENCE.md) | Complete API endpoint documentation | @server-agent | — |
| [DATA_FORMATS.md](./docs/DATA_FORMATS.md) | Supported file formats, validation rules | @core-agent | — |
| [CONTENT_DOWNLOADER.md](./docs/CONTENT_DOWNLOADER.md) | Download engine, file routing, task lifecycle | @downloader-agent | — |
| [DEPLOYMENT.md](./docs/DEPLOYMENT.md) | Binary, Docker, ARM setup, environment config | @build-agent | — |
| [DEV_SETUP.md](./docs/DEV_SETUP.md) | Local development environment, testing | @dev-agent | — |
| [RADIO_PLANNING.md](./docs/RADIO_PLANNING.md) | RTL-SDR roadmap (v2+) | @radio-agent | — |

---

## Build & Deployment

**Path**: `Cargo.toml`, `Dockerfile`, `.dockerignore`

### Release Profile (Cargo.toml)
```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

**Target**: ~15-20 MB binary

**Agent Task**: `@build-agent: Maintain Dockerfile, GitHub Actions for cross-compilation (x86_64, ARM64), and release artifacts`

**Current Status**:
- ✅ Root Cargo.toml with workspace
- ✅ Dependency management
- ⚠️ Dockerfile (not yet created)
- ⚠️ GitHub Actions (not yet created)

---

## Testing Strategy

**Implement**:
- ✅ Router tests (nexus-downloader) — `cargo test`
- ⚠️ Integration tests (API endpoints)
- ⚠️ End-to-end tests (downloader + server)

**Agent Task**: `@test-agent: Implement comprehensive test suite covering all modules and API endpoints`

---

## Dependencies & Minimal Stack

**Goal**: Minimize external crates, maximize stability

| Crate | Version | Purpose | Notes |
|-------|---------|---------|-------|
| tokio | 1.35 | Async runtime | Essential |
| axum | 0.7 | Web framework | Modern, minimal |
| serde | 1.0 | Serialization | Essential |
| tower-http | 0.5 | HTTP utilities | Static file serving |
| reqwest | 0.11 | HTTP client | Download engine |
| walkdir | 2 | File traversal | Content discovery |
| uuid | 1.6 | Task IDs | Task management |
| chrono | 0.4 | Timestamps | Metadata |
| tracing | 0.1 | Logging | Observability |

**Agent Task**: `@deps-agent: Review and audit all dependencies quarterly, remove unused crates, investigate lower-weight alternatives`

---

## Development Workflow

### Adding a New Endpoint
1. Define types in `nexus-core/src/types.rs`
2. Implement handler in `nexus-server/src/handlers.rs`
3. Add route to router in `nexus-server/src/main.rs`
4. Document in [API_REFERENCE.md](./docs/API_REFERENCE.md)
5. Test with `cargo test`

### Adding a New Content Type
1. Add to `ContentType` enum in `nexus-core/src/types.rs`
2. Add routing logic in `nexus-downloader/src/router.rs`
3. Implement validation in `nexus-core/src/validation.rs`
4. Update [DATA_FORMATS.md](./docs/DATA_FORMATS.md)
5. Test routing: `cargo test -p nexus-downloader`

### Adding Documentation
1. Create `.md` file in `docs/` directory
2. Reference in this AGENTS.md file
3. Link in README.md if user-facing
4. Update maintainer agent task

---

## Release Checklist

Before each release:
- [ ] All tests pass: `cargo test`
- [ ] Documentation updated and reviewed
- [ ] Changelog written
- [ ] Version bumped in `Cargo.toml`
- [ ] Binary builds: `cargo build --release`
- [ ] Binary size < 25 MB
- [ ] Docker image builds
- [ ] ARM64 cross-compilation succeeds
- [ ] GitHub release artifacts created

---

## Known Issues & TODOs

### High Priority
- [ ] Implement full HTTP download engine (DownloadEngine struct)
- [ ] Add progress tracking to downloads
- [ ] Create Vue/React UI frontend
- [ ] Implement Dockerfile
- [ ] Set up GitHub Actions for releases

### Medium Priority
- [ ] Add resume capability to downloads
- [ ] Implement book reader UI component
- [ ] Add map tile proxy/caching
- [ ] POI overlay map integration

### Low Priority (v0.2+)
- [ ] Full-text search for books/POIs
- [ ] Content validation (format conversion)
- [ ] Peer-to-peer sync
- [ ] User authentication

---

## Agent Task Summary

Use these tasks to delegate work:

```
@core-agent — Maintain nexus-core types, config, validation
@server-agent — Maintain API endpoints and request handlers
@downloader-agent — Implement download engine and file routing
@ui-agent — Build Vue/React frontend
@build-agent — Manage Dockerfile, GitHub Actions, releases
@test-agent — Write comprehensive tests
@docs-agent — Keep documentation synchronized with code
@radio-agent — Plan RTL-SDR integration (v2+)
@deps-agent — Audit and manage dependencies
```

---

## Communication & Questions

- **Documentation Questions**: Check [docs/](./docs/) first
- **Code Questions**: See relevant crate documentation in this file
- **Implementation Help**: Reference the module responsibility table above
- **Roadmap**: See README.md or this file's TODOs

---

**Last Updated**: 2026-07-17
**Maintained By**: Development Team
