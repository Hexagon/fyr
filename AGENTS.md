# AGENTS.md — Fyr Development Guide

This file coordinates code ownership, documentation rules, and delivery expectations.

## Canonical Documentation Policy

Authoritative documentation is limited to these files:

- [README.md](README.md)
- [docs-site/index.html](docs-site/index.html)
- [AGENTS.md](AGENTS.md)
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)

All other markdown under [docs/](docs/) should be treated as migration stubs only.
Do not add new standalone docs unless explicitly requested.

Audience guidance:
- [README.md](README.md): quick orientation and navigation hub.
- [docs-site/index.html](docs-site/index.html): install/deploy landing page.
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md): operator and end-user workflows.
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md): implementation and architecture.
- [CONTRIBUTING.md](CONTRIBUTING.md): PR process and required validation.

## Runtime Layout

Repository runtime is public-first:

- [public/static](public/static) - SPA build output
- [public/assets](public/assets) - static visual assets
- [public/data](public/data) - default content data

Config overrides:

- `DATA_DIR`
- `FYR_HOST`
- `FYR_PORT`
- `FYR_ADMIN_PASSWORD` — enables password-protected admin mode; mutating endpoints require a valid session
- `FYR_READONLY` — enables strict read-only mode; all mutating endpoints return 403 regardless of session state

## Core Security Function: Access Control

**This is a permanent, always-active architectural concern.** Every agent working on this codebase must be aware of it.

Fyr enforces read-only / admin access at the **Axum API middleware layer** (`crates/server/src/auth.rs`), not in the frontend. The frontend hiding of elements is a UX convenience only.

Rules to always respect:

1. **New mutating endpoints** (any `POST`, `PUT`, `PATCH`, `DELETE` that modifies server state) **must** be added to the `protected` sub-router in `crates/server/src/main.rs` so they inherit the `require_admin` middleware.
2. **Read-only endpoints** (pure `GET` listing/reading) may remain in the public router.
3. **Never bypass `require_admin`** in a handler or by restructuring the router in a way that removes the `route_layer`.
4. **`FYR_READONLY` is absolute** — it returns `403` even if a valid session cookie is present.
5. **Session tokens** are UUIDs stored in-memory (no persistence across restarts), set as HttpOnly cookies.
6. Frontend components that reveal sensitive system information (storage paths, disk usage) must also check `isAdminLocked()` from `crates/ui/frontend/src/services/auth.js`.

See `docs/developer/DEVELOPER_MANUAL.md § 3.5` for the full architecture description.

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
- Static serving for `/static`, `/assets`, `/data`, `/docs/books`
- Native ZIM reader API endpoints
- Auth middleware (`auth.rs`): session management, rate-limiting, `require_admin` middleware

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
- Books reading flows (EPUB + native ZIM)

## Development Workflow

When adding or changing behavior:

1. Implement code changes in the relevant crate.
2. Update user-facing behavior in [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md) and/or [docs-site/index.html](docs-site/index.html).
3. Update technical behavior in [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md).
4. If onboarding/quickstart changes, update [README.md](README.md) and/or [docs-site/index.html](docs-site/index.html).
5. If installation paths, platform setup, or deployment bootstrap steps change, update [docs-site/index.html](/docs-site/index.html).
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
