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
- Rust stable (Docker build currently uses `rust:1.75`)
- Node.js 20

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

## 6. Documentation Rules
1. Keep implementation details in developer docs, not user docs.
2. Keep transient delivery/status reports out of permanent docs.
3. Update docs in the same change set as endpoint or behavior changes.
4. Canonical docs are restricted to README, AGENTS, and user/developer manuals.

## 7. Building Documentation Artifacts
- Source script: `docs/build/build-manuals.js`
- Outputs:
  - `public/data/books/user-manual.md`
  - `public/data/books/developer-manual.md`

Run:
1. `cd docs/build`
2. `npm run build`

## 8. Current Known Gaps
- Download resume/range continuation is not yet implemented for interrupted transfers.
- CI checks for markdown/manual consistency are still basic and do not enforce cross-document semantic consistency.

## 9. Recommended Validation Sequence
Run from repository root unless noted:

1. `cargo test --workspace --all-targets`
2. `cargo check -p server`
3. `cd crates/ui/frontend && npm ci && npm run build`
4. `cd docs/build && npm ci && npm run build`
