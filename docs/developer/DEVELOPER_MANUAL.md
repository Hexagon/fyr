# Fyr Developer Manual

## 1. Overview
Fyr is a Rust workspace with a Vue frontend.

Workspace modules:
- `crates/types`: shared types and configuration.
- `crates/downloader`: task management and routing.
- `crates/server`: Axum HTTP API and static serving.
- `crates/ui/frontend`: Vue 3 application built into `public/static/`.
- `crates/server/src/ai`: Candle-powered GGUF model loading and assistant endpoints.

## 1.1 AI Integration with Candle

Fyr integrates GGUF models with native Rust inference tooling.

Dependencies:
- `candle-core`
- `candle-nn`
- `candle-transformers`

Model lifecycle:
- `ModelManager` scans `DATA_DIR/models` for `.gguf` files.
- `ModelLoader` validates GGUF magic bytes and parses metadata.
- Loader verifies tokenizer metadata before marking a model as ready.

Performance recommendation:
- Build release binaries with CPU tuning:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release -p server --bin fyr
```

Extending model support:
- Current integration uses GGUF metadata parsing plus quantized variable loading.
- Add architecture-specific runtime in `crates/server/src/ai/loader.rs` when introducing new generation backends.
- Keep unsupported architectures failing with explicit error messages instead of fallback panics.

## 2. Local Development
### Prerequisites
- Rust 1.70+
- Node.js 18+
- npm 9+

CI-pinned versions for parity:
- Rust stable (Docker build uses `rust:bookworm`)
- Node.js 24

### Build frontend
1. `cd crates/ui/frontend`
2. `npm ci` (or `npm install` for local iteration)
3. `npm run build`

### Build backend
1. From workspace root: `cargo build --release`
2. Run: `./target/release/fyr`

### Dev mode
- Frontend: `npm run dev`
- Backend: `cargo run -p server`

### Runtime environment overrides
- `DATA_DIR` (default `./public/data`)
- `FYR_HOST` (default `127.0.0.1`)
- `FYR_PORT` (default `8080`)

## 3. Docker

Reference image name in all docs/examples:

- `hexagon/fyr:latest`

Run prebuilt image:

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

Build locally and run:

```bash
docker build -t hexagon/fyr:latest .
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

Container expectations:

- App static assets live under `/app/public`.
- Writable content directory is mounted to `/data`.
- Startup sync overwrites `user-manual.md` and `developer-manual.md` in `DATA_DIR/books/` from image-bundled manuals.
- Healthcheck uses `GET /api/status`.
- Startup performs writable-path preflight checks and fails fast if `DATA_DIR` is not writable.
- Bind failures now include actionable diagnostics for `FYR_HOST` and `FYR_PORT`.

## 4. Active API Surface (Current)
Core endpoints:
- `GET /api/status`
- `GET /api/config`
- `GET /api/storage`
- `GET /api/content/maps`
- `GET /api/content/books`
- `GET /api/content/poi`
- `GET /api/content/models`
- `GET /api/content/misc`
- `POST /api/download`
- `DELETE /api/download/:task_id`
- `GET /api/download/:task_id/status`
- `GET /api/downloads`

AI assistant endpoints:
- `GET /api/models`
- `POST /api/models/upload`
- `POST /api/models/import`
- `POST /api/models/:filename/load`
- `DELETE /api/models/:filename/load`
- `GET /api/models/:filename/health`
- `GET /api/models/:filename/infer/stream`

Model upload/import flow:
- Frontend uploads `.gguf` files as multipart form data to `POST /api/models/upload`.
- The server sanitizes the filename, validates the `.gguf` extension and `GGUF` magic bytes, and writes the file into `DATA_DIR/inbox`.
- Frontend then calls `POST /api/models/import` with source `inbox` so `ModelManager` can move the file into `DATA_DIR/models`.
- Assistant and Content Manager now share this same upload-plus-import flow instead of relying on placeholder status text or manual pre-placement.

Current inference path:
- Fyr now has a real `qwen2` inference path based on `candle_transformers::models::quantized_qwen2::ModelWeights` plus `LogitsProcessor` sampling.
- The runtime currently requires tokenizer metadata embedded in the GGUF file.
- If tokenizer metadata is missing, model loading fails with a clear validation error.

Kiwix and ZIM endpoints:
- `GET /api/kiwix/status`
- `GET /api/reader/kiwix/capabilities`

Static content aliases:
- `GET /data/*path` (full data directory)
- `GET /docs/books/*path` (book-only alias used by embedded reader)

Kiwix integration notes:
- Frontend opens `.zim` with one click from Books and injects the selected URL into the embedded reader.
- Capabilities endpoint now reports `supports_direct_http_zim=true` and `zim_base_url=/docs/books`.
- Server exposes `Accept-Ranges: bytes` and CORS exposed headers (`content-length`, `content-range`, `accept-ranges`) for reader compatibility.
- ZIM content is read client-side via Kiwix HTTP range requests; no server-side ZIM content parsing endpoints are exposed.

Kiwix licensing and distribution notes:
- Fyr source code remains MIT-licensed at repository root.
- Embedded Kiwix assets under `public/kiwix-static/` are third-party code and retain their upstream licenses.
- Distribute these files with releases that include the embedded reader:
  - `public/kiwix-static/LICENSE-GPLv3.txt`
  - `public/kiwix-static/LICENSE-AGPLv3.txt`
  - `public/kiwix-static/THIRD_PARTY_NOTICES.txt`
- Keep bundle provenance current (upstream project reference, observed version markers, and local patch notes).

Download lifecycle notes:
- Download tasks are persisted to `DATA_DIR/download_tasks.json` using atomic write/rename.
- Persisted tasks are loaded on startup and immediately available through `GET /api/downloads`.
- URL downloads run in background workers with bounded retry attempts for transient network/server failures.
- Cancellation is cooperative: `DELETE /api/download/:task_id` marks the task as cancelled and worker state transitions preserve that terminal status.

## 5. Platform Support Guidance

Primary support targets:

- Linux `x86_64`
- Linux `aarch64` (including Raspberry Pi 64-bit)
- Windows `x86_64`

Recommendations:

- Prefer Docker multi-arch images for operational parity.
- Buildx example:

```bash
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t hexagon/fyr:latest \
  --push .
```

- For native releases, cross-compile with explicit Rust targets.
- Keep ARM runtime memory/storage constraints in mind for large map/ZIM archives.

## 6. Release Process (dev/main)

Branch model:
- `dev` is the integration branch and produces dev releases.
- `main` is the stable branch and produces stable releases only.

PR flow:
1. Merge feature branches into `dev`.
2. Validate and promote with a PR from `dev` to `main`.
3. Tag the `main` commit as `vX.Y.Z` for stable release.

GitHub workflows:
- `.github/workflows/release-dev.yml`
  - Trigger: merged PR into `dev` (or manual dispatch).
  - Runs full preflight (tests/check/build/docs/compliance).
  - Publishes Docker multi-arch dev images:
    - `hexagon/fyr:dev`
    - `hexagon/fyr:dev-<git-sha>`
- `.github/workflows/release.yml`
  - Trigger: push tag `v*.*.*` (or manual dispatch with `version`).
  - Verifies the release commit is reachable from `main`.
  - Runs full preflight and publishes Docker multi-arch stable images:
    - `hexagon/fyr:vX.Y.Z`
    - `hexagon/fyr:latest`
  - Creates a GitHub release with auto-generated notes.

Required repository secrets:
- `DOCKERHUB_USERNAME`
- `DOCKERHUB_TOKEN`

Operator release commands:
1. Prepare `dev` and ensure CI/workflows are green.
2. Merge promotion PR `dev -> main`.
3. Create and push stable tag:

```bash
git checkout main
git pull
git tag v0.4.1
git push origin v0.4.1
```

4. Confirm workflow `Stable Release` completed and images were published.

## 7. Documentation Rules
1. Keep implementation details in developer docs, not user docs.
2. Keep transient delivery/status reports out of permanent docs.
3. Update docs in the same change set as endpoint or behavior changes.
4. Canonical docs are restricted to README, AGENTS, and user/developer manuals.

## 8. Building Documentation Artifacts
- Source script: `docs/build/build-manuals.js`
- Outputs:
  - `public/data/books/user-manual.md`
  - `public/data/books/developer-manual.md`

Runtime note:
- These two files are treated as system-managed docs and are synchronized into `DATA_DIR/books/` at server startup.

Run:
1. `cd docs/build`
2. `npm run build`

## 9. Current Known Gaps
- Download resume/range continuation is not yet implemented for interrupted transfers.
- CI checks for markdown/manual consistency are still basic and do not enforce cross-document semantic consistency.

## 10. Recommended Validation Sequence
Run from repository root unless noted:

1. `cargo test --workspace --all-targets`
2. `cargo check -p server`
3. `cd crates/ui/frontend && npm ci && npm run build`
4. `cd docs/build && npm ci && npm run build`
5. Verify embedded reader license artifacts are present in release payloads:
  - `public/kiwix-static/LICENSE-GPLv3.txt`
  - `public/kiwix-static/LICENSE-AGPLv3.txt`
  - `public/kiwix-static/THIRD_PARTY_NOTICES.txt`
6. `cd docs/build && npm run verify:kiwix`

## 11. Kiwix Update Procedure (Required)
Use this procedure whenever `public/kiwix-static/` is refreshed from a new Kiwix release.

1. Fetch upstream release source/archive from `https://github.com/kiwix/kiwix-js`.
2. Update `public/kiwix-static/` with the required runtime subset for Fyr embedded reader.
3. Keep license artifacts in place:
  - `public/kiwix-static/LICENSE-GPLv3.txt`
  - `public/kiwix-static/LICENSE-AGPLv3.txt`
  - `public/kiwix-static/THIRD_PARTY_NOTICES.txt`
4. Update provenance metadata in `public/kiwix-static/KIWIX_SOURCE_MANIFEST.json`:
  - `upstream_release_tag`
  - `upstream_archive_url`
  - `source_code_url`
  - `bundle_runtime_markers`
  - `last_reviewed_utc`
5. If any local edits are applied to bundled Kiwix files, summarize changed paths/rationale in `THIRD_PARTY_NOTICES.txt` and in release notes.
6. Run compliance verification:

```bash
cd docs/build
npm run verify:kiwix
```

7. Run standard validation sequence and ensure release payload includes the full `public/kiwix-static/` compliance artifacts.
