# AGENTS.md — Fyr Development Guide

This file coordinates code ownership, documentation rules, and delivery expectations.

## Canonical Documentation Policy

Authoritative documentation is limited to these files:

- [README.md](README.md)
- [docs/site/index.html](docs/site/index.html)
- [AGENTS.md](AGENTS.md)
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

All other markdown under [docs/](docs/) should be treated as migration stubs only.
Do not add new standalone docs unless explicitly requested.

Audience guidance:
- [README.md](README.md): quick orientation and navigation hub.
- [docs/site/index.html](docs/site/index.html): install/deploy landing page.
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md): operator and end-user workflows.
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md): implementation and architecture.

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
- `FYR_GUEST_MESSAGING` — in password mode, lets guests post messages and create encrypted channels (see rule 8)
- `FYR_MESSAGING`, `FYR_MESSAGES_MAX_PER_CHANNEL`, `FYR_MESSAGES_MAX_AGE_DAYS` — Messages on/off and retention

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
7. Public endpoints must not leak host details (filesystem paths, bind host, disk capacity) in restricted modes: gate them with `auth::can_view_system_details` as `/api/status`, `/api/config`, `/api/storage`, content listings and download task responses do.
8. **Messaging is the one explicit exception to rule 1.** Exactly two write endpoints — `POST /api/channels` (register an encrypted channel) and `POST /api/channels/:id/messages` (post) — live in the separate `messaging_write` sub-router guarded by `auth::require_messaging_write`: `FYR_READONLY` → 403 always; open mode → allowed; password mode → admin session, or anyone when `FYR_GUEST_MESSAGING=true`, else 401. Handlers add per-IP token-bucket rate limits (in memory, IPv6 keyed by /64, never persisted). Encrypted channels additionally require the `X-Fyr-Channel-Key` header (constant-time check against the stored `sha256(auth_key)`). All messaging **moderation** (delete message, pin/unpin, delete channel) stays in the admin `protected` router. Do not add other mutating routes to the messaging sub-router, and never store or log message plaintext of encrypted channels, passphrases or channel keys server-side.

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
- Auth middleware (`auth.rs`): session management, rate-limiting, `require_admin` and `require_messaging_write` middleware
- Messages (`messaging/`): SQLite store (`DATA_DIR/messages.sqlite`), public board channel, encrypted-channel ciphertext storage, SSE streams, retention

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
- Messages page and client-side channel crypto (`src/modules/messaging/`; all crypto in `crypto.js`)

## Development Workflow

When adding or changing behavior:

1. Implement code changes in the relevant crate.
2. Update user-facing behavior in [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md) and/or [docs/site/index.html](docs/site/index.html).
3. Update technical behavior in [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md).
4. If onboarding/quickstart changes, update [README.md](README.md) and/or [docs/site/index.html](docs/site/index.html).
5. If installation paths, platform setup, or deployment bootstrap steps change, update [docs/site/index.html](docs/site/index.html).
6. Run validation (`cargo test --workspace --all-targets`, `cargo check -p server`, frontend build, docs build when relevant).

### Installer Scripts Constraint

`docs/site/install.sh` is piped via `curl | sh`, which bypasses the shebang line. The executing shell is `/bin/sh` (dash on Debian/Ubuntu, bash in POSIX mode on many others, busybox sh on Alpine). **The script must be POSIX sh-compatible at all times.**

Forbidden bashisms:
- `[[ ... ]]` — use `[ ... ]` instead
- `command &>/dev/null` — use `command >/dev/null 2>&1` instead
- `=~` regex operator — use `case` or `grep` instead
- `==` inside `[ ]` — use `=` instead
- `function name()` — use `name()` instead
- Arrays (`arr=(...)`) — use space-separated strings

`docs/site/install.ps1` has no such constraint (PowerShell is always the executor).

## Docker and Platform Expectations

- Docker image name for docs/examples: `hexagon/fyr:latest`.
- Keep [Dockerfile](Dockerfile) aligned with runtime layout in [public](public).
- Ensure both `x86_64` and `aarch64` paths are documented when build or deployment changes.

## Release Checklist

These steps are in addition to the per-PR validation in the Development Workflow section above.

- bump `version` in the workspace `Cargo.toml`; update `crates/ui/frontend/package.json` `version` to match; update any version references in [docs/site/index.html](docs/site/index.html)
- manual docs reviewed
- docker build succeeds

## Build Environment

- Rust: use stable toolchain (Docker uses `rust:bookworm`).
- Node.js: CI workflows use Node `24`.
- Prefer matching CI toolchain versions locally when troubleshooting build drift.

### Automatic Frontend Build

`crates/server/build.rs` automatically runs `npm ci && npm run build` in `crates/ui/frontend/` before compiling the Rust server binary. This means a single command builds everything:

```sh
cargo build -p server          # debug build
cargo build --release -p server  # release build
```

The frontend output goes to `public/static/` (configured in `vite.config.js`). The build script only re-runs when frontend source files change, thanks to `cargo:rerun-if-changed` directives.
