# Fyr Developer Manual

## 1. Overview
Fyr is a Rust workspace with a Vue frontend.

> **Project status:** Fyr is currently in **preview**; prefer additive and backward-compatible changes where practical.

Documentation map:
- Product overview and quick navigation: [README.md](../../README.md)
- Installation/deployment site: [docs/site/index.html](../../docs/site/index.html)
- End-user behavior reference: [User Manual](../user/USER_MANUAL.md)
- Contributor workflow and validation requirements: [CONTRIBUTING.md](../../CONTRIBUTING.md)
- Governance and ownership boundaries: [AGENTS.md](../../AGENTS.md)

Workspace modules:
- `crates/types`: shared types and configuration.
- `crates/downloader`: task management and routing.
- `crates/server`: Axum HTTP API and static serving.
- `crates/ui/frontend`: Vue 3 application built into `public/static/`.
- `crates/server/src/ai`: Candle-powered GGUF model loading and assistant endpoints.

The Tools page (`crates/ui/frontend/src/pages/Tools.vue`) mixes client-only and API-backed behavior. Unit converters, Base64, and ROT13 run entirely in-browser, while AES and hash/checksum operations call backend endpoints (`POST /api/tools/aes`, `POST /api/tools/hash`) implemented in `crates/server/src/handlers.rs`.

Downloader timeout centralization:
- URL download request timeout is sourced from persisted app settings at `settings.modules.downloads.request_timeout_seconds`.
- On server startup and each `PUT /api/settings`, the server applies this value to `DownloadManager` for future tasks.
- Valid range is clamped to `30..=86400` seconds; default is `300` seconds.

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

- On Windows PowerShell:

```powershell
$env:RUSTFLAGS="-C target-cpu=native"
cargo build --release -p server --bin fyr
Remove-Item Env:RUSTFLAGS
```

- Use `target-cpu=native` only for local/self-hosted builds that will run on the same CPU family. For portable binaries and published images, keep the generic target settings.

### CPU optimization and thread pool sizing

- GGUF weights are loaded via `memmap2` (`crates/server/src/ai/loader.rs`) instead of being read fully into a `Vec<u8>`. The OS pages weight data in on demand and can evict it under memory pressure, which matters on RAM-constrained boards such as a Raspberry Pi.
- At startup, `configure_ai_runtime()` in `crates/server/src/main.rs` logs detected CPU SIMD capabilities (NEON/fp16/dotprod/fcma on aarch64, AVX2/FMA on x86_64), comparing what the running binary was compiled with against what the hardware actually supports. This check runs on the real deployment hardware at process startup, so it stays accurate even when the Docker image itself was built under QEMU emulation for a foreign architecture (see Docker section below).
- The same function sizes Candle's CPU thread pool via `CANDLE_NUM_THREADS`/`RAYON_NUM_THREADS`, defaulting to `std::thread::available_parallelism()` (cgroup/quota-aware, unlike a naive core count). Override with `FYR_AI_THREADS=<n>`, or set `CANDLE_NUM_THREADS`/`RAYON_NUM_THREADS` directly for full manual control; Fyr will not overwrite either variable if already set.
- Each inference run logs a `tracing::info!` summary of prefill and decode throughput (tokens and tok/s, separately) from `spawn_quantized_inference()` in `crates/server/src/ai/manager.rs`, regardless of how the run ends (success, error, or early stop), to make performance regressions and slow hardware easy to spot in server logs.
- For aarch64 self-builds targeting known hardware, use the Dockerfile's `RUST_TARGET_FEATURES` build arg (e.g. `+dotprod` for Raspberry Pi 5 / Cortex-A75 and newer) to compile in optional ARMv8.2+ kernels. Baseline NEON is always enabled on aarch64 and does not require this flag.

### Inference hardening

The streaming inference loop in `crates/server/src/ai/manager.rs` applies two server-side guards before sending tokens to the client:

1. **Repetition detection** — `is_repeating()` compares the last 32 generated tokens with the 32 tokens before them. If they match, generation stops immediately. This prevents models from getting stuck in a loop when temperature is low or the prompt is ambiguous.

2. **ChatML stop markers** — `first_role_marker_index()` catches `<|im_start|>` and `<|im_end|>` tokens in addition to plain-text role prefixes such as `ASSISTANT:` and `USER:`. Generation stops and the visible prefix is flushed when any marker is detected.

`<think>…</think>` blocks are passed through to the client as-is. The Vue frontend (`crates/ui/frontend/src/pages/Assistant.vue`) splits incoming tokens into response text and think-block content using `parseThinkAndText()`. The assistant now shows a generic thinking placeholder immediately on send, then upgrades that block with live `<think>` content if the model emits it.

The browser streaming client in `crates/ui/frontend/src/services/api.js` reads the SSE response body with `fetch()` and a `ReadableStream` reader rather than relying on `EventSource`. This avoids the chunked final-render behavior seen in some environments and keeps stop/error handling explicit.

### Recommended model files

Fyr's inference runtime currently supports GGUF files for the **Qwen2**, **Llama**, and **Phi-3 / Phi-3.5** architecture families when tokenizer metadata is embedded. For Phi-3/Phi-3.5 specifically, a sidecar tokenizer JSON may be required for reliable generation quality.

| Tier | Suggested model | Quantization | Approx. size | Notes |
|---|---|---|---|---|
| Small | `Qwen2.5-1.5B-Instruct` | Q8_0 | ~1.7 GB | Fast/simple answers, workable on 4 GB Raspberry Pi 5 systems |
| Standard | `Qwen2.5-3B-Instruct` | Q6_K | ~2.6 GB | Recommended default for Raspberry Pi 5 RAG usage |
| Large | `Qwen2.5-7B-Instruct` | Q4_K_M | ~4.5 GB | Intended for 8 GB Raspberry Pi 5 systems |
| Extra large | `Qwen2.5-14B-Instruct` | Q4_K_M | ~9.8 GB | Desktop-grade RAG quality on 16 GB+ systems |
| Extra large (alt) | `Qwen2.5-7B-Instruct` | Q8_0 | ~8.5 GB | Smaller desktop alternative when 14B is too heavy |
| Llama alternative | `Llama-3.2-3B-Instruct` | Q4_K_M | ~2.0 GB | Good cross-domain instruct model; some downloads require Hugging Face license acceptance |
| Phi alternative | `Phi-3.5-mini-instruct` | Q4_K_M | ~2.4 GB | Compact reasoning-oriented option with `phi3` GGUF architecture |

GGUF files for these models are commonly published on [Hugging Face](https://huggingface.co/models?library=gguf). Examples used by Fyr's curated catalog include `Qwen/Qwen2.5-3B-Instruct-GGUF`, `bartowski/Llama-3.2-3B-Instruct-GGUF`, and `bartowski/Phi-3.5-mini-instruct-GGUF`.

Default assistant profiles used by Fyr:

| Mode | temperature | max_tokens | Notes |
|------|-------------|------------|-------|
| Precise | 0.1 | 512 | Focused, factual answers |
| Balanced | 0.2 | 512 | Default, concise and reliable |
| Creative | 0.7 | 1024 | More elaborate, varied responses |

- `num_ctx = 2048` — auto-upgrade to `8192` when host RAM > 16 GB
- Manual override: set `settings.modules.assistant.high_ram_context = true` or `settings.modules.assistant.num_ctx` to an explicit value

Conversation context: the SSE endpoint accepts an optional `history` query parameter — a JSON array of `{"role":"user"|"assistant","text":"..."}` objects. The frontend passes the last six messages. The backend formats the full conversation as a multi-turn ChatML prompt before inference.

Catalog file:

- Fyr seeds `DATA_DIR/curated-content.json` from the bundled `public/data/curated-content.json` when the file is missing.
- Existing `DATA_DIR/curated-content.json` files are preserved so operators can maintain a local curated list across upgrades.
- The frontend content manager reads `/data/curated-content.json`, shows curated entries when a category is empty, and keeps them as supplemental recommendations when local files already exist.
- Curated entries may include an optional `download_url` field that queues a direct download from the content manager UI.

Models with a built-in reasoning mode (Qwen3, DeepSeek-R1, etc.) are supported. Their `<think>…</think>` output is displayed in the UI as a collapsible "Thinking" block above the live response stream.

Prompt formatting is architecture-specific:
- `qwen2` uses ChatML-style `<|im_start|>...<|im_end|>` prompts.
- `llama` uses the Llama 3 instruct header template with `<|start_header_id|>` and `<|eot_id|>` markers.
- `phi3` uses the Phi chat template with `<|system|>`, `<|user|>`, `<|assistant|>`, and `<|end|>` delimiters.

Extending model support:
- Current integration uses GGUF metadata parsing plus quantized variable loading.
- Add architecture-specific runtime and prompt formatting in `crates/server/src/ai/loader.rs` and `crates/server/src/ai/manager.rs` when introducing new generation backends.
- Keep unsupported architectures failing with explicit error messages instead of fallback panics.

## 2. Local Development
### Prerequisites
- Rust 1.70+
- Node.js 24
- npm 9+

CI-pinned versions for parity:
- Rust stable (Docker build uses `rust:bookworm`)

### Build frontend
1. `cd crates/ui/frontend`
2. `npm ci` (or `npm install` for local iteration)
3. `npm run build`

### Build backend
1. From workspace root: `cargo build --release -p server`
2. Run: `./target/release/fyr`

### Dev mode
- Frontend: `npm run dev`
- Backend: `cargo run -p server`
- The Vite dev server proxies `/api`, `/data`, and `/docs` to the backend. Override the backend target with `FYR_DEV_PROXY_TARGET` when the server runs somewhere other than the local default.

### Runtime environment overrides
- `DATA_DIR` (default `./public/data`)
- `FYR_HOST` (default `127.0.0.1`)
- `FYR_PORT` (default `8080`)
- `FYR_ADMIN_PASSWORD` (optional; enables password-protected admin mode)
- `FYR_READONLY` (optional; enables strict read-only mode; all mutating endpoints return 403)
- `FYR_DEV_PROXY_TARGET` (optional Vite dev proxy target; useful when the backend runs in Docker or on another host)

## 3. Docker

Reference image name in all docs/examples:

- `hexagon/fyr:latest`

For complete Docker installation and configuration (including persistence, bind mounts, and platform-specific notes), see the canonical installation guide at [docs/site/index.html](../site/index.html) or [fyr.guide/#installation](https://fyr.guide/#installation).

Container expectations:

- App static assets live under `/app/public`.
- Docker image builds frontend assets during image build and bundles generated files under `/app/public/static`.
- Writable content directory is mounted to `/data`.
- Startup sync overwrites `user-manual.md` and `developer-manual.md` in `DATA_DIR/books/` from image-bundled manuals.
- Healthcheck uses `GET /api/status`.
- Startup performs writable-path preflight checks and fails fast if `DATA_DIR` is not writable.
- Bind failures now include actionable diagnostics for `FYR_HOST` and `FYR_PORT`.

### Multi-arch builds and CPU features

- `docker/setup-qemu-action` + `buildx` are used in CI to produce `linux/arm64` images on `x86_64` GitHub-hosted runners. This is QEMU-emulated user-mode execution of `rustc`/`cargo`, not true cross-compilation (host and target triple both resolve to `aarch64-unknown-linux-gnu` from Cargo's point of view), so build correctness is unaffected. The main practical costs are much slower CI build times for the `arm64` leg and no reliable build-time signal for optional ARMv8.2+ CPU feature probing under emulation.
- The Dockerfile accepts an optional `RUST_TARGET_FEATURES` build arg (default empty) that is threaded into `RUSTFLAGS` as `-C target-feature=<value>`. Use it only for self-builds targeting known hardware, e.g.:

```bash
docker build --build-arg RUST_TARGET_FEATURES=+dotprod -t fyr:rpi5 .
```

- Never set `RUST_TARGET_FEATURES` when building the published multi-arch `hexagon/fyr:*` tags — it would silently break older aarch64 boards (Raspberry Pi 3/4, Cortex-A53/A72) that do not implement `dotprod`/`i8mm`/`fp16` and would crash with an illegal-instruction fault. Baseline NEON is part of the mandatory ARMv8-A instruction set and is always available regardless of this flag.
- Because build-time feature negotiation is unreliable under emulation, Fyr instead detects and logs real CPU capabilities at process startup on the actual deployment hardware (`configure_ai_runtime()` in `crates/server/src/main.rs`; see § 1.1 above). Check the startup logs after deploying to a new device to confirm whether a rebuild with `RUST_TARGET_FEATURES` would help.

Published image strategy:

- Default tags are optimized multi-arch manifests assembled from architecture-specific tuned images:
  - `hexagon/fyr:latest` and `hexagon/fyr:vX.Y.Z`
  - `hexagon/fyr:dev`
- Compatibility-first legacy tags are published for older hardware:
  - `hexagon/fyr:legacy` and `hexagon/fyr:vX.Y.Z-legacy` (multi-arch manifest)
  - `hexagon/fyr:pc-legacy` and `hexagon/fyr:vX.Y.Z-pc-legacy` (generic x86_64)
  - `hexagon/fyr:rpi-legacy` and `hexagon/fyr:vX.Y.Z-rpi-legacy` (generic arm64)
  - Dev equivalents: `hexagon/fyr:dev-legacy`, `hexagon/fyr:dev-pc-legacy`, `hexagon/fyr:dev-rpi-legacy` (plus SHA-suffixed variants)

Use default tags only when the target CPU definitely supports the required instructions. For mixed fleets or older boards/PCs, prefer the legacy tags.

## 3.5 Access Control Architecture

Fyr implements a two-level access control system controlled entirely by environment variables at startup. The restriction is enforced at the Axum API layer — the frontend also adapts its UI, but that is a secondary UX concern, not the security boundary.

### Modes

| Mode | Env var | Effect |
|------|---------|--------|
| Open (default) | _(none)_ | All endpoints accessible |
| Password-protected | `FYR_ADMIN_PASSWORD=<pass>` | Mutating endpoints require a valid session cookie |
| Strict read-only | `FYR_READONLY=true` | All mutating endpoints return `403 Forbidden`; no login possible |

### Implementation (`crates/server/src/auth.rs`)

- **`AuthManager`** — in-memory struct holding:
  - A `Mutex<HashSet<String>>` of live session tokens (UUIDs).
  - A `Mutex<HashMap<String, RateLimitEntry>>` for per-IP rate limiting.
- **`require_admin` middleware** — Axum `from_fn_with_state` middleware applied via `route_layer` to a sub-router containing all mutating routes. It checks:
  1. If `FYR_READONLY` → return `403`.
  2. If no password configured → pass through (open mode).
  3. Otherwise → validate the `fyr_session` cookie value against `AuthManager::tokens`.
- **Session cookie** — `HttpOnly; Path=/; SameSite=Strict`. Not accessible to JavaScript.
- **Rate limiting** — 10 failed attempts per 5-minute window per client IP. The rate-limit key is always the real TCP peer address (`ConnectInfo<SocketAddr>`), so clients cannot bypass limits by spoofing `X-Forwarded-For` headers. When Fyr runs behind a reverse proxy, all requests share the proxy's IP; in that case the proxy should handle rate limiting.
- **`AuthConfig`** lives in `crates/types/src/config.rs` and is populated from env vars in `Config::default()`.

### Adding new protected endpoints

Add the route to the `protected` `Router` in `create_router` in `crates/server/src/main.rs`. The `route_layer(admin_mw)` call automatically covers all routes in that sub-router.



### Auth endpoints (always public)
- `GET /api/auth/status` — returns `{ readonly, requires_auth, authenticated }`. Use this to determine the current access mode.
- `POST /api/auth/login` — body: `{ "password": "..." }`. Returns `Set-Cookie: fyr_session=<token>; HttpOnly; Path=/; SameSite=Strict` on success. Returns `429 Too Many Requests` when rate-limited.
- `POST /api/auth/logout` — clears the session cookie and revokes the token.

### Core read-only endpoints (always public)
- `GET /api/status`
- `GET /api/config`
- `GET /api/storage`
- `GET /api/settings`
- `GET /api/content/maps`
- `GET /api/maps/tiles/:filename/metadata` — MBTiles metadata as JSON
- `GET /api/maps/tiles/:filename/:z/:x/:y` — MBTiles tile endpoint (XYZ convention)
- `GET /api/content/books`
- `GET /api/content/poi`
- `GET /api/content/models`
- `GET /api/content/misc`
- `GET /api/content/:type/:filename/download` — browser file download endpoint used by Content Manager
- `GET /api/download/:task_id/status`
- `GET /api/downloads`

### Admin-only endpoints (require auth when `FYR_ADMIN_PASSWORD` is set; always blocked when `FYR_READONLY`)
- `PUT /api/settings`
- `DELETE /api/content/:type/:filename` — permanently delete a content file from disk
- `POST /api/download`
- `DELETE /api/download/:task_id`
- `DELETE /api/download/:task_id/dismiss`
- `POST /api/import/upload`
- `POST /api/import/download/:filename`
- `POST /api/models/upload`
- `POST /api/models/import`
- `POST /api/models/:filename/load`
- `DELETE /api/models/:filename/load`

AI read-only endpoints (always public):
- `GET /api/models`
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
- Loader tokenizer resolution order is: `<model>.tokenizer.json`, `<model>.json`, `tokenizer.json` in the model directory, then GGUF tokenizer metadata fallback.
- Phi/Phi-3.5 models should prefer the official tokenizer JSON sidecar when available.
- If tokenizer metadata is missing, model loading fails with a clear validation error.

Unified Library API endpoints (read-only, always public):
- `GET /api/library/books/:filename` — Unified book metadata (title, author, format, file size, MIME type, TOC/search availability)
- `GET /api/library/books/:filename/toc` — Unified table of contents (EPUB nav/ncx, ZIM articles, PDF outline, Markdown headings)
- `GET /api/library/books/:filename/search?q=...&limit=20` — Unified full-text search (ZIM articles, EPUB spine items, Markdown lines)

The library module (`crates/server/src/library.rs`) implements format-agnostic extraction for metadata, TOC, and search. Each format handler (EPUB, ZIM, PDF, Markdown) is isolated in its own helper section within the module.

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
- Content Manager's per-file **Download** action calls `/api/content/:type/:filename/download`.

Native ZIM integration notes:
- Frontend reader logic is split into format-specific modules under `crates/ui/frontend/src/modules/reader/` (`useEpubReader`, `useMarkdownReader`, `usePdfReader`, `useZimReader`) and orchestrated by `useUnifiedReader`.
- `.zim` archives are opened through the native ZIM module, which fetches metadata/capabilities from server endpoints and renders article HTML in a sandboxed iframe (`srcdoc`) with same-origin navigation bridged via `postMessage`.
- Native ZIM rendering preserves sanitized archive root attributes (`html`/`body` class, id, lang, dir, style) so archive-authored layout systems (for example Wikipedia main-page mosaic rules) can apply without Fyr-specific fallback selectors.
- The ZIM sandbox document does not inject Fyr fallback theme CSS; rendering uses archive-provided CSS plus browser defaults.
- Server-side article resolution uses the Rust `zim` crate and returns article payloads through `/api/reader/zim/:filename/native/article`.
- Blob/resource lookup is available via `/api/reader/zim/:filename/native/content/*path`, and rewritten asset links in the frontend target this endpoint.
- Native mode is always active for `.zim` archives. The `FYR_ZIM_NATIVE_EXPERIMENTAL` toggle has been removed.

Reader shell layout notes:
- `Books.vue` uses a unified reader viewport where each format surface owns its own scroll behavior.
- Read mode uses a single consolidated top toolbar that merges title, filename, status, ZIM search controls, and compact ZIM metadata chips (adapter/archive/article).
- The Library panel auto-collapses whenever a book is opened; collapsed mode hides the panel entirely so the reader owns the full stage, and the state remains session-local (no persistence).
- Legacy dynamic iframe height synchronization for ZIM and cross-surface resize plumbing were removed to avoid nested scrollbar complexity.

Licensing and distribution notes:
- Fyr source code remains MIT-licensed at repository root.
- Native ZIM support is implemented directly in server code using Rust dependencies in `crates/server/Cargo.toml`.

Download lifecycle notes:
- Download tasks are persisted to `DATA_DIR/download_tasks.json` using atomic write/rename.
- Persisted tasks are loaded on startup and immediately available through `GET /api/downloads`.
- URL downloads and local file imports both run in background workers with bounded retry/copy status updates.
- Cancellation is cooperative: `DELETE /api/download/:task_id` marks the task as cancelled and worker state transitions preserve that terminal status.
- Startup cleanup prunes stale `*.part` temp files from `DATA_DIR/inbox` (older than 24h).

## 4. Platform Support Guidance

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

## 5. Release Process (dev/main)

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
  - Publishes Docker optimized multi-arch dev images:
    - `hexagon/fyr:dev`
    - `hexagon/fyr:dev-<git-sha>`
  - Publishes Docker optimized architecture-specific dev images:
    - `hexagon/fyr:dev-amd64-avx2`
    - `hexagon/fyr:dev-<git-sha>-amd64-avx2`
    - `hexagon/fyr:dev-arm64-dotprod`
    - `hexagon/fyr:dev-<git-sha>-arm64-dotprod`
  - Publishes Docker legacy dev images:
    - `hexagon/fyr:dev-legacy`
    - `hexagon/fyr:dev-<git-sha>-legacy`
    - `hexagon/fyr:dev-pc-legacy`
    - `hexagon/fyr:dev-<git-sha>-pc-legacy`
    - `hexagon/fyr:dev-rpi-legacy`
    - `hexagon/fyr:dev-<git-sha>-rpi-legacy`
- `.github/workflows/release.yml`
  - Trigger: push tag `v*.*.*` (or manual dispatch with `version`).
  - Verifies the release commit is reachable from `main`.
  - Runs full preflight and publishes Docker optimized multi-arch stable images:
    - `hexagon/fyr:vX.Y.Z`
    - `hexagon/fyr:latest`
  - Publishes architecture-specific optimized build artifacts used to assemble the optimized manifests.
  - Publishes Docker legacy stable images:
    - `hexagon/fyr:vX.Y.Z-legacy`
    - `hexagon/fyr:legacy`
    - `hexagon/fyr:vX.Y.Z-pc-legacy`
    - `hexagon/fyr:pc-legacy`
    - `hexagon/fyr:vX.Y.Z-rpi-legacy`
    - `hexagon/fyr:rpi-legacy`
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
git tag vX.Y.Z
git push origin vX.Y.Z
```

4. Confirm workflow `Stable Release` completed and images were published.

## 6. Documentation Rules
1. Keep implementation details in developer docs, not user docs.
2. Keep transient delivery/status reports out of permanent docs.
3. Update docs in the same change set as endpoint or behavior changes.
4. Canonical docs are restricted to `README.md`, `docs/site/index.html`, `AGENTS.md`, `docs/user/USER_MANUAL.md`, `docs/developer/DEVELOPER_MANUAL.md`, and `CONTRIBUTING.md`.

## 7. Building Documentation Artifacts
- Source script: `docs/build/build-manuals.js`
- Outputs:
  - `public/data/books/user-manual.md`
  - `public/data/books/developer-manual.md`

Runtime note:
- These two files are treated as system-managed docs and are synchronized into `DATA_DIR/books/` at server startup.

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
4. `cd docs/build && npm run build`
5. Validate native ZIM flow by opening a `.zim` file in Books and confirming article payload retrieval.

## 10. Native ZIM Reader Notes
1. Keep server-side reader contracts stable:
  - `/api/reader/zim/:filename/meta`
  - `/api/reader/zim/:filename/capabilities`
  - `/api/reader/zim/:filename/native/article`
  - `/api/reader/zim/:filename/native/content/*path`
2. Maintain clean-room implementation boundaries: all ZIM reader logic must be implemented directly in Fyr's Rust server code using the `zim` crate. Do not bundle or wrap third-party reader JavaScript bundles.
3. Validate representative archives after reader changes and monitor unsupported compression/edge-case failures.
