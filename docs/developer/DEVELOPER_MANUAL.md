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

### Build frontend
1. `cd crates/ui/frontend`
2. `npm install`
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
- `GET /api/download/:task_id/status`
- `GET /api/downloads`

AI assistant endpoints:
- `GET /api/models`
- `POST /api/models/import`
- `POST /api/models/:filename/load`
- `DELETE /api/models/:filename/load`
- `GET /api/models/:filename/health`
- `GET /api/models/:filename/infer/stream`

Kiwix and ZIM endpoints:
- `GET /api/kiwix/status`
- `GET /api/reader/kiwix/capabilities`
- `GET /api/zim/:filename/meta`
- `GET /api/zim/:filename/main`
- `GET /api/zim/:filename/content/*path`

Static content aliases:
- `GET /data/*path` (full data directory)
- `GET /docs/books/*path` (book-only alias used by embedded reader)

Kiwix integration notes:
- Frontend opens `.zim` with one click from Books and injects the selected URL into the embedded reader.
- Capabilities endpoint now reports `supports_direct_http_zim=true` and `zim_base_url=/docs/books`.
- Server exposes `Accept-Ranges: bytes` and CORS exposed headers (`content-length`, `content-range`, `accept-ranges`) for reader compatibility.

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

## 7. Building Documentation EPUBs
- Source script: `docs/build/build-manuals.js`
- Outputs:
  - `public/data/books/user-manual.epub`
  - `public/data/books/developer-manual.epub`

Run:
1. `cd docs/build`
2. `npm install`
3. `node build-manuals.js`

## 8. Current Known Gaps
- Server-side ZIM reader compatibility for very large archives still needs improvement.
- Download engine progression and resumable downloads are not fully implemented.
- CI checks for markdown/manual consistency are not yet added.
