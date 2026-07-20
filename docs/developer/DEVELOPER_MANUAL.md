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

### Inference hardening

The streaming inference loop in `crates/server/src/ai/manager.rs` applies three guards before sending tokens to the client:

1. **Repetition detection** — `is_repeating()` compares the last 32 generated tokens with the 32 tokens before them. If they match, generation stops immediately. This prevents models from getting stuck in a loop when temperature is low or the prompt is ambiguous.

2. **`<think>` block stripping** — `strip_think_blocks()` removes content between `<think>` and `</think>` tags before emitting any text. If the closing tag has not yet arrived the output is held back at the opening tag. This prevents internal reasoning from models such as Qwen3 and DeepSeek-R1 from reaching the chat UI.

3. **ChatML stop markers** — `first_role_marker_index()` now catches `<|im_start|>` and `<|im_end|>` tokens in addition to plain-text role prefixes such as `ASSISTANT:` and `USER:`. Generation stops and the visible prefix is flushed when any marker is detected.

### Recommended model files

Fyr's inference runtime requires GGUF files for the **Qwen2** architecture with embedded tokenizer metadata.

| Use case | Suggested model | Quantization |
|---|---|---|
| Low memory (≤2 GB, Raspberry Pi) | `Qwen2.5-0.5B-Instruct` | Q4_K_M |
| General use | `Qwen2.5-1.5B-Instruct` | Q4_K_M or Q6_K |
| Higher quality (≥8 GB RAM) | `Qwen2.5-3B-Instruct` | Q6_K or Q8_0 |

GGUF files for these models are published under the **Qwen** organisation on [Hugging Face](https://huggingface.co/Qwen). Example repository: `Qwen/Qwen2.5-1.5B-Instruct-GGUF`.

**Avoid thinking-mode variants** (e.g. `Qwen3`, `DeepSeek-R1`) for everyday use. Fyr filters their `<think>` reasoning output, but the extra tokens increase latency without adding visible value. Use standard instruction-tuned (`*-Instruct`) variants instead.

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
- The Vite dev server proxies `/api`, `/data`, and `/docs` to the backend. Override the backend target with `FYR_DEV_PROXY_TARGET` when the server runs somewhere other than the local default.

### Runtime environment overrides
- `DATA_DIR` (default `./public/data`)
- `FYR_HOST` (default `127.0.0.1`)
- `FYR_PORT` (default `8080`)
- `FYR_DEV_PROXY_TARGET` (optional Vite dev proxy target; useful when the backend runs in Docker or on another host)

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
- Docker image builds frontend assets during image build and bundles generated files under `/app/public/static`.
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
- `DELETE /api/content/:type/:filename` — permanently delete a content file from disk (type: maps, books, poi, models, misc)
- `POST /api/download`
- `DELETE /api/download/:task_id`
- `DELETE /api/download/:task_id/dismiss` — remove a download task from the list (cancels first if still active)
- `GET /api/download/:task_id/status`
- `GET /api/downloads`
- `POST /api/import/upload`
- `POST /api/import/download/:filename`

AI assistant endpoints:
- `GET /api/models`
- `POST /api/models/upload`
- `POST /api/models/import`
- `POST /api/models/:filename/load`
- `DELETE /api/models/:filename/load`
- `GET /api/models/:filename/health`
- `GET /api/models/:filename/infer/stream`

Model upload/import flow:
- Model uploads are initiated from the Content Manager models section; the Assistant links there instead of uploading directly.
- Content Manager uploads `.gguf` files as multipart form data to `POST /api/import/upload`.
- The server sanitizes the filename, validates the `.gguf` extension and `GGUF` magic bytes, and writes the file into `DATA_DIR/inbox`.
- Frontend then calls `POST /api/import/download/:filename`, allowing `DownloadManager` to route the staged file into `DATA_DIR/models`.

Generic local import flow:
- Content Manager uploads supported files as multipart form data to `POST /api/import/upload`.
- Server writes uploaded files to `DATA_DIR/inbox` and returns detected content type metadata.
- Frontend then calls `POST /api/import/download/:filename` to enqueue a `DownloadSource::LocalFile` task.
- `DownloadManager` runs local import workers with the same task lifecycle and persistence model used by URL downloads, and the frontend refreshes both download tasks and content listings when task states change.

Current inference path:
- Fyr now has a real `qwen2` inference path based on `candle_transformers::models::quantized_qwen2::ModelWeights` plus `LogitsProcessor` sampling.
- The runtime currently requires tokenizer metadata embedded in the GGUF file.
- If tokenizer metadata is missing, model loading fails with a clear validation error.

Reader and ZIM endpoints:
- `GET /api/reader/capabilities`
- `GET /api/reader/open/:filename`
- `GET /api/reader/zim/:filename/meta`
- `GET /api/reader/zim/:filename/capabilities`
- `GET /api/reader/zim/:filename/native/article`
- `GET /api/reader/zim/:filename/native/content/*path`

Static content aliases:
- `GET /data/*path` (full data directory)
- `GET /docs/books/*path` (book-only alias used by reader integrations)

Native ZIM integration notes:
- Frontend opens `.zim` through the unified reader module and fetches metadata/capabilities from native server endpoints.
- Server-side article resolution uses the Rust `zim` crate and returns article payloads through `/api/reader/zim/:filename/native/article`.
- Blob/resource lookup is available via `/api/reader/zim/:filename/native/content/*path`.
- Native mode is always active for `.zim` archives. The `FYR_ZIM_NATIVE_EXPERIMENTAL` toggle has been removed.

Licensing and distribution notes:
- Fyr source code remains MIT-licensed at repository root.
- Native ZIM support is implemented directly in server code using Rust dependencies in `crates/server/Cargo.toml`.

Download lifecycle notes:
- Download tasks are persisted to `DATA_DIR/download_tasks.json` using atomic write/rename.
- Persisted tasks are loaded on startup and immediately available through `GET /api/downloads`.
- URL downloads and local file imports both run in background workers with bounded retry/copy status updates.
- Cancellation is cooperative: `DELETE /api/download/:task_id` marks the task as cancelled and worker state transitions preserve that terminal status.
- Startup cleanup prunes stale `*.part` temp files from `DATA_DIR/inbox` (older than 24h).

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
  - Runs full preflight (tests/check/build/docs).
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
5. Validate native ZIM flow by opening a `.zim` file in Books and confirming article payload retrieval.

## 11. Native ZIM Reader Notes
1. Keep server-side reader contracts stable:
  - `/api/reader/zim/:filename/meta`
  - `/api/reader/zim/:filename/capabilities`
  - `/api/reader/zim/:filename/native/article`
  - `/api/reader/zim/:filename/native/content/*path`
2. Maintain clean-room implementation boundaries (no third-party reader bundle code).
3. Validate representative archives after reader changes and monitor unsupported compression/edge-case failures.
