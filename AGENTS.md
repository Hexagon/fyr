# AGENTS.md — Fyr Development Guide

This file coordinates code ownership, documentation rules, and delivery expectations.

## Canonical Documentation Policy

Authoritative documentation is limited to these files:

- [README.md](README.md)
- [INSTALL.md](INSTALL.md)
- [AGENTS.md](AGENTS.md)
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)

All other markdown under [docs/](docs/) should be treated as migration stubs only.
Do not add new standalone docs unless explicitly requested.

## Runtime Layout

Repository runtime is public-first:

- [public/static](public/static) - SPA build output
- [public/kiwix-static](public/kiwix-static) - embedded Kiwix bundle
- [public/assets](public/assets) - static visual assets
- [public/data](public/data) - default content data

Config overrides:

- `DATA_DIR`
- `FYR_HOST`
- `FYR_PORT`

## Module Responsibilities

### Core Module: types
Path: [crates/types/src](crates/types/src)

Responsibilities:

- Shared types and validation
- Config and directory initialization
- Environment-based runtime overrides

### Server Module: server
Path: [crates/server/src](crates/server/src)

Responsibilities:

- Axum API handlers and routing
- Static serving for `/static`, `/kiwix`, `/assets`, `/data`, `/docs/books`
- ZIM API compatibility endpoints

### Downloader Module: downloader
Path: [crates/downloader/src](crates/downloader/src)

Responsibilities:

- Download queue lifecycle
- Content routing by file type
- Progress/status persistence behavior

### UI Module: frontend
Path: [crates/ui/frontend](crates/ui/frontend)

Responsibilities:

- Vue SPA pages and API integration
- Maps rendering (MapLibre/PMTiles)
- Books reading flows (EPUB + embedded ZIM)

## Development Workflow

When adding or changing behavior:

1. Implement code changes in the relevant crate.
2. Update user-facing behavior in [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md).
3. Update technical behavior in [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md).
4. If onboarding/quickstart changes, update [README.md](README.md).
5. If installation paths, platform setup, or deployment bootstrap steps change, update [INSTALL.md](INSTALL.md).
6. Run validation (`cargo test --workspace --all-targets`, `cargo check -p server`, frontend build, docs build when relevant).

## Docker and Platform Expectations

- Docker image name for docs/examples: `hexagon/fyr:latest`.
- Keep [Dockerfile](Dockerfile) aligned with runtime layout in [public](public).
- Ensure both `x86_64` and `aarch64` paths are documented when build or deployment changes.

## Release Checklist

- `cargo test --workspace --all-targets`
- `cargo check -p server`
- `cd crates/ui/frontend && npm run build`
- `cd docs/build && npm run build`
- manual docs reviewed
- docker build succeeds

## Build Environment

- Rust: use stable toolchain (Docker uses `rust:bookworm`).
- Node.js: CI workflows use Node `24`.
- Prefer matching CI toolchain versions locally when troubleshooting build drift.
