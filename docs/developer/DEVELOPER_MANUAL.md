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

Prompt budgeting (`crates/server/src/ai/prompt.rs`): the prompt is fitted into `num_ctx` with the loaded tokenizer before inference. `input_token_budget()` reserves room for `max_tokens` (at most half the window); `fit_prompt()` always keeps the system prompt and question, then library sources (up to two thirds of the remaining room when there is history; the last one that does not fit is trimmed with `trim_text_to_tokens()`), then as much recent history as fits, dropping the oldest turns first and never keeping an orphaned assistant reply at the start. Tail-trimming the token ids (`trim_context_window`, which cuts the system prompt) is now only a last resort when even the bare prompt exceeds the budget.

### Ask your library (retrieval-augmented answers)

`/api/models/:filename/infer/stream` exists as `POST` with a JSON body (`{prompt, temperature?, max_tokens?, num_ctx?, history: [{role, text}], use_library?, sources_limit?}`; used by the UI so long conversations are not limited by URL length; at most the last 200 history messages are considered) and as the original `GET` with query params (`history` as a JSON-encoded string). Both are public: inference changes no server state. Both accept two optional library params:

| Param | Default | Notes |
|---|---|---|
| `use_library` | `false` | `true` enables retrieval; any value other than `true`/`false` is `400` |
| `sources_limit` | `4` | Clamped to `1..=8` |

Flow (`ai_infer_stream` in `handlers.rs`):

1. `ModelManager::ensure_inference_ready()` runs first, so an unloaded model still returns `409` (validation-only models `500`) without doing any retrieval. The old path (no `use_library`) is unchanged.
2. `ai::retrieval::retrieve()` runs on the blocking pool inside `tokio::time::timeout(time_budget + 1.5 s)`; a timeout or panic degrades to "no sources".
3. `ModelManager::infer_stream(filename, InferRequest { .., library })` fits the prompt (above) and returns `InferStream { sources, tokens }`. `sources` lists only the passages that survived budgeting, numbered as the model sees them.
4. The SSE stream emits `event: sources` (JSON array) before the first `token`, then `token` events and `done`:

```json
[{"n":1,"title":"Gustav Vasa","source":"wikipedia_sv.zim","link":"/books?file=wikipedia_sv.zim&path=Gustav_Vasa"},
 {"n":2,"title":"Survival Guide › Water","source":"survival.md","link":"/books?file=survival.md&q=boil%20water"}]
```

`link` follows the Books deep-link contract: `/books?file=<filename>&path=<zim article path>` for ZIM articles (the path is the same normalized entry URL the native article endpoint accepts), `/books?file=<filename>&q=<phrase>` for Markdown/EPUB, where the phrase is a keyword bigram or keyword that occurs in the passage. Only bare filenames and archive-internal paths are ever emitted; no host paths, in any mode.

Prompt-injection hardening: library passages and the catalog summary come from user-supplied files, so `prompt::neutralize_control_markers` breaks chat-template delimiters (`<|`, `|>`, `[INST]`, `<s>`, …) in titles, filenames and text before they enter the system block — tokenizers would otherwise turn e.g. `<|im_end|>` inside an article into a real special token and let it open a new turn. The sources header also tells the model to treat excerpts as quoted material, never as instructions.

Retrieval (`crates/server/src/ai/retrieval.rs`, pure helpers unit-tested):

- **Keywords:** lowercase, split on non-alphanumeric characters, drop tokens under 3 characters (2-digit numbers kept) and English/Swedish stop words, max 10.
- **ZIM candidates** (per archive, max 6 articles): exact `get_by_path` binary-search lookups for phrase variants (`Gustav_Vasa`, `Gustav_vasa`, as typed) in the `C` and `A` namespaces; a prefix scan over the `X/listing/titleOrdered/v1` article listing via `lower_bound`; then keyword-in-title matches from the global search's cached title index (`search::cached_title_keyword_matches`, shared so questions never rescan an indexed archive; requesting it schedules the background index build for archives up to 250k entries), falling back to a linear title scan only for archives with at most 50 000 entries while no index is cached. Redirects are resolved; only HTML articles are read. Large archives are never walked end to end.
- **Markdown:** heading sections → paragraphs. **EPUB:** OPF spine documents → block-level paragraphs. PDFs are skipped.
- **Text:** `html_to_blocks()` drops `script`/`style`/`head`/…, splits at block elements, reuses `library::strip_html_tags`, decodes entities and removes `[12]`-style reference markers so they cannot be confused with the model's `[n]` citations.
- **Scoring:** paragraphs are chunked to `max_passage_chars` (900), scored with BM25 (k1 = 1.2, b = 0.75, statistics over the candidate set) plus a title bonus, grouped per document (best chunk + 0.25 × second), and the top `sources_limit` documents become sources combining their two best chunks in document order.
- **Budgets** (`RetrievalLimits::default()`): 2.5 s wall clock, 8 ZIM archives, 64 Markdown/EPUB files, 4 MiB per file/article/chapter, 32 MiB total bytes read. ZIM archives are probed first (cheap, most relevant). Each file is wrapped in `catch_unwind`; broken files are skipped with a warning.

The system block appends the numbered sources (`[n] Title — file` + excerpt) and instructs the model to cite `[n]`, to cite only listed numbers, and to say when the sources do not contain the answer. With no passages it says so instead.

Frontend: `services/citations.js` validates the `sources` payload (internal `/…` links only) and turns `[n]` / `[1, 2]` citations into `<a class="citation">` links (skipping code spans, fenced blocks, markdown links and reference definitions) before `marked` + DOMPurify. `services/api.js` routes SSE frames through `dispatchInferenceEvent()` (`onSources`, `onToken`, `onDone`). `Assistant.vue` persists the **Use library** toggle (`fyr_assistant_use_library`) and preset (`fyr_assistant_preset`) in `localStorage`, renders sources as `router-link` chips and routes citation clicks through the router. Tests: `citations.test.js`, `citations.render.test.js`, `api.test.js`.

Catalog file:

- Fyr seeds `DATA_DIR/curated-content.json` from the bundled `public/data/curated-content.json` when the file is missing.
- Existing `DATA_DIR/curated-content.json` files are preserved so operators can maintain a local curated list across upgrades.
- The frontend content manager reads `/data/curated-content.json`, shows curated entries when a category is empty, and keeps them as supplemental recommendations when local files already exist.
- Curated entries may include an optional `download_url` field that queues a direct download from the content manager UI.
- Curated entries with a `download_url` may also carry an optional `sha256` (64 hex chars); the content manager passes it to `POST /api/download` so the finished file is verified.

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

### Frontend unit tests
- `cd crates/ui/frontend && npm test` runs Vitest (`happy-dom` environment) over `src/**/*.test.js`. CI runs it as its own **Frontend Unit Tests** check (separate from **Frontend Build**), followed by `npm audit --omit=dev --audit-level=high`, so a vulnerable production dependency fails the PR.
- Keep testable logic in plain modules under `src/services/` or `src/modules/` (pure functions, composables) and colocate `*.test.js` next to them. Vue pages should stay thin glue around those modules.

### Web app manifest
- `crates/ui/frontend/public/manifest.webmanifest` (served at `/static/manifest.webmanifest`, `start_url`/`scope` `/`) plus PNG icons in `public/assets/icons/` (192, 512, maskable 512, 180 apple-touch) rendered from `fyr-icon.svg`.
- There is intentionally no service worker: Fyr is usually reached over plain HTTP on a LAN, where browsers refuse to register service workers, and the app is useless without its server anyway.

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
- `FYR_GUEST_MESSAGING` (optional; with `FYR_ADMIN_PASSWORD`, guests may post messages / create channels)
- `FYR_TRUSTED_PROXIES` (optional; reverse proxy IPs/CIDRs whose `X-Forwarded-For` is honoured for client IPs)
- `FYR_MESSAGING` (default `true`; `false` disables Messages), `FYR_MESSAGES_MAX_PER_CHANNEL` (default `1000`, clamped 10–100000), `FYR_MESSAGES_MAX_AGE_DAYS` (default `90`, clamped 1–3650)
- `FYR_DEV_PROXY_TARGET` (optional Vite dev proxy target; useful when the backend runs in Docker or on another host)

## 3. Docker

Reference image name in all docs/examples:

- `hexagon/fyr:latest`

For complete Docker installation and configuration (including persistence, bind mounts, and platform-specific notes), see the canonical installation guide at [docs/site/index.html](../site/index.html) or [fyr.guide/#installation](https://fyr.guide/#installation).

Container expectations:

- App static assets live under `/app/public`.
- Docker image builds frontend assets during image build and bundles generated files under `/app/public/static`.
- The final Rust image build uses `FYR_USE_PREBUILT_FRONTEND=1` so `crates/server/build.rs` reuses those prebuilt assets instead of running `npm ci` again in the Rust builder stage.
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
- **Rate limiting** — 10 failed attempts per 5-minute window per client IP. The key comes from `auth::client_ip`: the real TCP peer address (`ConnectInfo<SocketAddr>`), unless that peer is listed in `FYR_TRUSTED_PROXIES` (IPs/CIDRs, parsed into `types::TrustedProxies`). Only then is `X-Forwarded-For` read, from the right, taking the first hop that is not itself a trusted proxy; client-supplied hops further left are never used, and unparsable values fall back to the peer. IPv4-mapped IPv6 peers are matched as IPv4. Messaging rate limits and stream caps (`messaging::routes::ClientIp`) use the same function.
- **`AuthConfig`** lives in `crates/types/src/config.rs` and is populated from env vars in `Config::default()`.
- **`can_view_system_details`** — decides whether a caller may see host details: everyone in open mode, only valid admin sessions in password mode, nobody in read-only mode. Public endpoints use it to redact output for other callers:
  - `/api/status`, `/api/config`, `/api/storage` and `/api/reader/zim/:filename/meta` return empty strings for filesystem paths (and `server_host`); `/api/storage` also omits `free_*`/`capacity_*` while keeping per-category usage.
  - `/api/content/*` listings return the bare filename as `file_path`, and `/api/downloads` / `/api/download/:task_id/status` reduce `LocalFile` source paths to the filename.
  New public endpoints that expose paths or disk details must apply the same check.

### Adding new protected endpoints

Add the route to the `protected` `Router` in `create_router` in `crates/server/src/main.rs`. The `route_layer(admin_mw)` call automatically covers all routes in that sub-router.

### Messaging write exception (`require_messaging_write`)

Messaging is the single, deliberate exception to "mutating endpoints are admin-only". Only `POST /api/channels` and `POST /api/channels/:id/messages` are mounted in the `messaging_write` sub-router, layered with `auth::require_messaging_write` (`route_layer`) and a 32 KiB body limit:

| Mode | Result |
|------|--------|
| `FYR_READONLY` | `403` always (even with a session or `FYR_GUEST_MESSAGING`) |
| Open (no password) | allowed |
| Password, valid admin session | allowed |
| Password, guest, `FYR_GUEST_MESSAGING=true` | allowed |
| Password, guest otherwise | `401` |

`auth::messaging_write_access` implements the table and also feeds `can_post` in `GET /api/messages/config`; `auth::has_admin_access` (same rule as `can_view_system_details`) feeds `can_moderate`. Moderation routes are merged into `protected` and therefore use `require_admin`. Handlers apply per-IP token buckets (`messaging::limits::RateLimiter`; posts: burst 8, then 1 per 4 s; channel creation: burst 3, then 1 per 60 s) keyed by the TCP peer address (`ConnectInfo`, never `X-Forwarded-For`), with IPv6 truncated to /64; nothing is persisted and the tables are bounded (4096 IPs).



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
- `GET /api/maps/tiles/:filename/:z/:x/:y` — MBTiles tile endpoint (XYZ convention). Tiles and metadata are read through `mbtiles::MbtilesPool`, which keeps up to 8 idle read-only SQLite connections plus the cached tile format per archive, keyed by file size + mtime so a replaced archive is reopened instead of served from stale handles.
- `GET /api/content/books`
- `GET /api/content/poi`
- `GET /api/content/models`
- `GET /api/content/misc`
- `GET /api/content/:type/:filename/download` — browser file download endpoint used by Content Manager. Streams via `tower_http::services::ServeFile` (no in-memory buffering), supports `Range`/`If-Range`/`HEAD`, and sets `Content-Disposition: attachment` with an ASCII `filename` plus RFC 5987 `filename*`.
- `GET /api/download/:task_id/status`
- `GET /api/downloads`
- `GET /api/search?q=&limit=&kinds=` — global offline search (see § 11). Returns filenames and frontend routes only, never host paths, so no `can_view_system_details` gating is needed.

### Messages endpoints

One model for the built-in public channel (id `public`, "Board") and encrypted channels (id = 32 lowercase hex chars):

Public (reads; encrypted channels need `X-Fyr-Channel-Key: <hex auth_key>`):
- `GET /api/messages/config` — `{ enabled, can_post, can_moderate, guest_messaging, server_salt (hex, 32 bytes), public_channel_id, limits: { max_text_chars, max_nick_chars, max_ciphertext_bytes, default_ttl_days, max_ttl_days, max_messages_per_channel, max_age_days } }`. Always mounted (reports `enabled: false` when `FYR_MESSAGING=false`; all other messaging routes are then not mounted).
- `GET /api/channels/:id/messages?since=&before=&limit=&category=` — `{ channel_id, kind: "public"|"encrypted", messages: [...ascending], pinned: [...], has_more }`. `since` returns the oldest `limit` messages with `id > since` (catch-up); otherwise the newest `limit` (optionally `id < before`). `limit` defaults to 100, max 500. `pinned` is filled for the public channel's first page only. Expired messages are never returned.
- `GET /api/channels/:id/stream` — SSE. Events: `ready` (sent first; clients then catch up with `?since=`), `message` (row JSON), `deleted` (`{id}`), `pinned` (row JSON), `resync` (subscriber lagged: catch up), `channel_deleted` (stream ends). Keep-alive comments every 20 s, `X-Accel-Buffering: no`, closed after 30 min (clients reconnect). Capped at 256 concurrent streams and 8 per client IP (`429`).

Status codes for channel access: malformed id `400`, unknown channel `404` (UI offers "create"), missing/malformed key `401`, wrong key `403`. There is **no** channel listing endpoint.

Messaging writes (`require_messaging_write`, rate-limited):
- `POST /api/channels` — `{ channel_id, verifier }` where `verifier = hex(sha256(auth_key))`. `201`, `409` if it exists (never overwritten), `507` when 5000 encrypted channels exist.
- `POST /api/channels/:id/messages` — public channel body `{ nick (1–40 chars), device_tag (4–16 lowercase hex), text (1–2000 chars), category?: info|need|offer|alert, lat?, lng? (both or neither), ttl_days? (1–365, default 30) }`; encrypted channel body exactly `{ nonce (base64, 24 bytes), ciphertext (base64, 16–8192 bytes) }`. Unknown fields are rejected (`422`), so plaintext can never be stored for an encrypted channel. Control characters and bidi overrides are rejected in public nick/text. Returns the stored row (`201`).

Admin-only (in `protected`):
- `DELETE /api/channels/:id/messages/:msg_id` (any channel)
- `POST /api/channels/public/messages/:msg_id/pin` — `{ pinned: bool }` (public channel only)
- `DELETE /api/channels/:id` — encrypted channels only; cascades to its messages.

Row shape: `{ id, channel_id, created_at, expires_at, pinned }` plus either the public fields (`nick, device_tag, category?, text, lat?, lng?`) or `nonce`/`ciphertext` (base64). Timestamps are Unix milliseconds; ids are monotonically increasing and double as the `since`/`before` cursor.

### Messages storage and retention (`crates/server/src/messaging/`)

- `store.rs` — SQLite at `DATA_DIR/messages.sqlite` (created `0600`, WAL, `secure_delete`), versioned migrations (`schema_version` table + ordered `MIGRATIONS`), tables `meta` (random 32-byte `server_salt`, generated with `ring::rand` on first start), `channels` (`id, kind, verifier, created_at, last_activity`; the `public` row is seeded by migration 1) and `messages`. One connection behind a `Mutex`; every DB call runs on the blocking pool.
- Retention: public messages have `expires_at` (default 30 days, max 365; pinned never expire); encrypted messages expire after `FYR_MESSAGES_MAX_AGE_DAYS`. Each insert trims its channel to the newest `FYR_MESSAGES_MAX_PER_CHANNEL` unpinned messages. A Tokio task (`Messaging::spawn_pruner`, every 10 min) deletes expired messages, re-applies per-channel caps, enforces a global cap of 200 000 messages, drops encrypted channels that have had no messages and no activity for 7 days (re-creatable, ids are deterministic) and prunes rate-limit tables.
- `limits.rs` — token buckets and the SSE `StreamLimiter` (permit released on drop).
- `routes.rs` — handlers and the three route groups (`public_routes`, `write_routes`, `moderation_routes`); `router_tests.rs` covers the access matrix.
- Live fan-out uses one `tokio::sync::broadcast` channel (capacity 512); each SSE connection forwards events for its channel into an `mpsc` and ends when the client disconnects.

### Channel crypto specification (v1)

All client crypto is in `crates/ui/frontend/src/modules/messaging/crypto.js`, using `@noble/hashes` and `@noble/ciphers` 2.4.0 (pure JS; WebCrypto `crypto.subtle` is unavailable on plain-HTTP origins, `crypto.getRandomValues` is used for randomness):

```
name'       = lowercase(trim(NFKC(name)))
passphrase' = NFKC(passphrase)                      (no trimming / case folding)
salt        = SHA-256("fyr-channel/v1" || server_salt[32 bytes] || UTF-8(name'))
master      = scrypt(UTF-8(passphrase'), salt, N = 2^15, r = 8, p = 1, dkLen = 32)
channel_id  = hex(HKDF-SHA256(ikm = master, salt = empty, info = "fyr/v1/id",   L = 16))
auth_key    =     HKDF-SHA256(ikm = master, salt = empty, info = "fyr/v1/auth", L = 32)
enc_key     =     HKDF-SHA256(ikm = master, salt = empty, info = "fyr/v1/enc",  L = 32)
verifier    = hex(SHA-256(auth_key))                 (registered with the server)
payload     = JSON { v: 1, nick, device_tag, text, sent_at, [category], [lat, lng] },
              UTF-8, right-padded with spaces to a multiple of 64 bytes
message     = XChaCha20-Poly1305(key = enc_key, nonce = 24 random bytes,
                                 aad = UTF-8(channel_id), plaintext = payload)
```

- KDF parameters are versioned in `KDF_VERSIONS`; never edit an existing version, add a new one. scrypt v1 takes ~0.3 s in Node/Chromium on a desktop x86 CPU (expect ~1 s on a mid-range phone); progress is shown while deriving.
- The server never receives the passphrase or `enc_key`; it sees `channel_id` and `auth_key` (header) and stores only `sha256(auth_key)`, compared in constant time. Message plaintext of encrypted channels is never sent to the server.
- Decrypted payloads are validated and cleaned (lengths, device tag format, control/bidi characters) before display; message text is rendered via `linkify()` segments and Vue text interpolation, never as HTML.
- "Remember on this device" stores `{ name, channelId, authKey, encKey, version }` in `localStorage` (never the passphrase). Share links use the URL fragment (`/messages#join=<name>[&pass=<passphrase>]`), which the page strips with `history.replaceState` after reading.
- Tests: `crypto.test.js` has RFC 7914 scrypt and draft-irtf-cfrg-xchacha known answers, a v1 derivation known answer cross-checked with Python `hashlib`, normalization, tamper, wrong-passphrase and AAD tests.
- Threat model: protects against passive Wi-Fi sniffing and server-disk disclosure; does not protect against an active attacker who modifies page code on plain HTTP (use HTTPS), and anyone with the passphrase can post under any nickname. A captured `channel_id` allows offline passphrase guessing at scrypt cost, so the UI asks for long passphrases.

### Messages frontend

- `pages/Messages.vue` (route `/messages`), `modules/messaging/`: `model.js` (row → view model, merge/expiry/alerts), `format.js` (linkify, day grouping, Maps deep links `/maps?lat=&lng=&zoom=15&label=`), `storage.js` (guarded `localStorage`: nickname, device id/tag, remembered channels, read markers, own message ids, dismissed alerts), `stream.js` (fetch-based SSE so the key header can be sent; `ready`-triggered catch-up; exponential backoff with jitter; 400/401/403/404 terminal), `boardFeed.js` (one shared Board stream for the nav badge, Board view and `AlertBanner.vue` on Home).
- The UI hides the composer when `can_post` is false and moderation controls when `can_moderate` is false; the server enforces both.

### Admin-only endpoints (require auth when `FYR_ADMIN_PASSWORD` is set; always blocked when `FYR_READONLY`)
- `PUT /api/settings`
- `DELETE /api/content/:type/:filename` — permanently delete a content file from disk
- `POST /api/download`
- `DELETE /api/download/:task_id`
- `DELETE /api/download/:task_id/dismiss`
- `POST /api/download/:task_id/retry` — re-queue a failed/cancelled URL task, resuming from its kept partial (200 with the task, 404 unknown, 409 not retriable: local import, still active, completed, or server shutting down)
- `POST /api/import/upload`
- `POST /api/import/download/:filename`
- `POST /api/models/upload`
- `POST /api/models/import`
- `POST /api/models/:filename/load`
- `DELETE /api/models/:filename/load`

Download routing: URL downloads stream into `inbox/<task_id>-<filename>.part` (unique per task, so concurrent downloads of the same filename cannot corrupt each other) and are then moved to their content folder with `DownloadManager::move_file`, which falls back to copy + delete when `rename` fails across filesystems (for example when `maps/` or `books/` is a separate Docker volume).

`ModelManager::import_model` and `load_model` accept only bare `.gguf` filenames (`validate_model_filename`; anything with directory components or `..` returns `ModelError::InvalidFilename` → `400`). GGUF parsing/weight loading and the inbox→models move run on Tokio's blocking pool (`spawn_blocking`) so a multi-second model load never stalls other requests.

AI read-only endpoints (always public):
- `GET /api/models`
- `GET /api/models/:filename/health`
- `GET|POST /api/models/:filename/infer/stream` (POST takes a JSON body; optional `use_library` / `sources_limit`; source links carry bare filenames only, see § 1.1)

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

Handlers that do synchronous disk or archive work — content listings (including EPUB/ZIM title extraction), `/api/status` file counts, `/api/storage` directory walks, the assistant's content catalog summary, library metadata/TOC/search and all native ZIM article/content/search endpoints — run that work through `handlers::run_blocking` (a `spawn_blocking` wrapper). New handlers that walk directories, parse archives or scan ZIM indexes must do the same so tiles and UI requests keep flowing on low-core devices such as a Raspberry Pi.

The library module (`crates/server/src/library.rs`) implements format-agnostic extraction for metadata, TOC, and search. Each format handler (EPUB, ZIM, PDF, Markdown) is isolated in its own helper section within the module.

Reader and ZIM endpoints:
- `GET /api/reader/capabilities`
- `GET /api/reader/open/:filename`
- `GET /api/reader/zim/:filename/meta`
- `GET /api/reader/zim/:filename/capabilities`
- `GET /api/reader/zim/:filename/native/article`
- `GET /api/reader/zim/:filename/native/content/*path`

Static content aliases:
- `GET /static/*path` — SPA build output. Content-hashed bundles under `/static/js/` and `/static/css/` are served with `Cache-Control: public, max-age=31536000, immutable` (`static_cache_headers` in `main.rs`), as are content-hashed Vite assets under `/static/assets/` (`name-<hash>.ext`, e.g. MapLibre's worker; `has_content_hash`). `index.html`, unhashed files copied from `crates/ui/frontend/public/` (`fyr-icon.svg`, `icons/`) and all other responses keep the global `no-store` policy.
- MapLibre 6 runs its renderer in a separate module worker. `Maps.vue` imports `maplibre-gl/dist/maplibre-gl-worker.mjs?url` and calls `maplibregl.setWorkerUrl()`, so the worker ships as a local asset (no CDN, works offline). Vite asset names keep their extension (`assets/[name]-[hash][extname]`) because browsers refuse module workers served with a non-JavaScript MIME type.
- `GET /data/maps/*path`, `GET /data/books/*path`, `GET /data/poi/*path` and `GET /data/curated-content.json` — the only published parts of `DATA_DIR`. `settings.json`, `download_tasks.json`, `messages.sqlite`, `inbox/`, `models/` and `misc/` are intentionally not served; unmatched `/data/*` and `/api/*` paths return `404` instead of the SPA `index.html`.
- `GET /docs/books/*path` (book-only alias used by reader integrations)
- Content Manager's per-file **Download** action calls `/api/content/:type/:filename/download`.

Maps frontend notes (`crates/ui/frontend/src/pages/Maps.vue` + `src/modules/maps/`):
- `Maps.vue` is glue around plain modules, each with colocated Vitest tests:
  - `geo.js` — haversine/path length, distance/duration/byte formatting, longitude wrapping, bounds normalisation (`[w,s,e,n]` arrays or MBTiles `"w,s,e,n"` strings, antimeridian-aware), `pickMapForPoint` (smallest containing bounds wins).
  - `coords.js` — decimal/DMS formatting and `parseCoordinate` (decimal pairs, decimal commas, `geo:` URIs, hemisphere letters as prefix or suffix, DMS/DM with or without symbols). Returns `null` for anything ambiguous or out of range.
  - `deepLink.js` — the deep-link contract (below), `buildMapsLink()` for other pages.
  - `bounds.js` — `createBoundsResolver` caches map coverage per file version: PMTiles header `minLon/minLat/maxLon/maxLat` (read client-side with the `pmtiles` library, a small range request) or MBTiles metadata `bounds` from `GET /api/maps/tiles/:filename/metadata`. No backend change was needed.
  - `mapFiles.js` / `mapSources.js` — archive URLs and `loadMapSource(map, filename)`, which attaches PMTiles (tile types 1/6 vector, 2–5 raster, with fallback) or MBTiles (tile endpoint). MBTiles vector layer ids are read from the spec `json` metadata row or a top-level `vector_layers` row.
  - `style.js` — base style, default vector layer builder (source-layer name mapping for Protomaps/Shortbread/OpenMapTiles schemas), layer-toggle groups, and overlay layers (measure, POI points/lines/polygons, live track).
  - `poi.js` — FeatureCollection normalisation, filename helpers (`uniquePoiFilename`, `slugifyFilename`).
  - `gpx.js` — `parseGpx` (DOMParser, namespace-agnostic local-name matching, GPX 1.0/1.1; `wpt`→Point, each `trkseg`→LineString, `rte`→LineString with `gpxType:'rte'`; elevation as 3rd coordinate, times in `properties.coordTimes`) and `toGpx` (GPX 1.1, XML-escaped, invalid XML chars stripped, polygons skipped and counted).
  - `tracking.js` / `useTrackRecorder.js` — pure track accumulation (accuracy ≤ 75 m, ≥ 3 m steps, monotonic time) and an app-wide `watchPosition` recorder (`getTrackRecorder()`) that survives SPA navigation, requests a screen wake lock, and snapshots the in-progress/unsaved track to `localStorage` (`fyr.maps.trackRecording`) for recovery. Insecure contexts (`window.isSecureContext === false`) are detected up front with an explanatory message.
  - `browser.js` — clipboard copy with an `execCommand` fallback (the async Clipboard API is unavailable on plain-http LAN origins), client-side downloads.
- Deep-link contract: `/maps?map=<filename>&lat=<lat>&lng=<lng>&zoom=<z>&label=<text>`, all optional (`lon`/`long` and `z` accepted as aliases). With `lat`+`lng` a labelled marker is shown (label rendered via `textContent`, never HTML; max 200 chars) and the view centres there (`zoom` default 14, clamped 0–22). Without `map`, the current map is kept if it covers the point, else the most specific installed map whose bounds contain it, else the current/first map. An unknown `map` shows an error banner. Selecting a map, picking one for a point, and *Go to coordinate* update the URL with `router.replace`; the page compares a normalised key of the query to avoid re-applying its own writes. Other pages should build links with `buildMapsLink({ lat, lng, zoom, label, map })` from `src/modules/maps/deepLink.js`.
- Access control: POI saves go through the admin-protected `PUT /api/poi/:filename`. The page hides create/place/remove/save (including saving recorded tracks) when `isAdminLocked()` is true; viewing POI files, importing GPX for display, measuring, recording and downloading remain available to guests.
- POI files render points (circle + label), lines (casing + line) and polygons (translucent fill). Clicking a point/line/polygon with no tool active opens a popup (built with `textContent`) with its name, length for lines, the coordinate in both formats and copy buttons.

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
- Startup cleanup prunes stale `*.part` temp files from `DATA_DIR/inbox` (older than 24h), except partials of URL tasks that are still `resumable`.

Resumable URL downloads (`crates/downloader/src/manager.rs`, `fetch_url_to_partial`):
- `POST /api/download` takes `{ "url": "...", "sha256": "<hex>"? }`; a malformed `sha256` returns `400 {"message": ...}`. `normalize_sha256` lowercases it and it is stored on the task.
- `DownloadTask` gained `sha256`, `etag`, `last_modified`, `resumable` and `resumed_from` (all `#[serde(default)]`, so older `download_tasks.json` files still load) and the `verifying` status.
- Every attempt checks the partial file `inbox/<task_id>-<filename>.part`. When it is non-empty the request carries `Range: bytes=<len>-` plus `If-Range` with the strong ETag (weak `W/` ETags are skipped) or else `Last-Modified`, both persisted from the `200` response that started the file.
- `206` with a `Content-Range` starting at `<len>` (and an unchanged ETag) appends; `200` truncates and restarts (no Range support, or If-Range mismatch); `416` accepts the partial only when its size equals the `Content-Range: bytes */<total>` total (or the persisted `total_bytes`), otherwise truncates and restarts. A stream that ends short of the announced length is treated as a network failure.
- Network/stream errors and retriable HTTP codes (408/429/5xx) are retried in-task with backoff (`MAX_DOWNLOAD_ATTEMPTS` consecutive failures). An attempt that grows the partial beyond its largest size so far resets the counter, so a link that keeps making progress is resumed until done, while a server that ignores Range and drops at the same offset still runs out of attempts.
- On final failure the partial is kept and the task is `failed` + `resumable` (non-retriable HTTP errors keep an existing partial too; disk write errors and checksum mismatches delete it). User cancel deletes the partial; dismissing an idle task deletes its partial; a running worker deletes its own partial when it sees the cancel flag (workers hold their own flag `Arc`, so dismiss removing the map entry cannot hide the cancellation).
- `retry_task` holds the `cancel_flags` and `tasks` locks together (always in that order) so the "no worker running" check and the re-queue to `queued` are atomic.
- With a `sha256`, the finished partial is hashed with `ring` on the blocking pool (`verifying`) before it is renamed into the inbox; a mismatch fails the task and removes the file.
- Restart recovery (`recover_interrupted_tasks`) marks non-terminal URL tasks `failed` with "interrupted by server restart – retry to resume", keeps the partial and sets `bytes_downloaded` to its on-disk size; local-import partials are still removed.

Graceful shutdown (`crates/server/src/main.rs`):
- `axum::serve(...).with_graceful_shutdown` listens for Ctrl+C everywhere and SIGTERM on Unix (`tokio::signal`).
- On the signal, `DownloadManager::shutdown` first sets a shutdown flag and wakes workers through a `Notify` (so workers blocked on a stalled stream or a retry backoff stop at once). URL workers flush their partial and record themselves as `failed` + `resumable` ("interrupted by server shutdown – retry to resume"); local imports fail and drop their temp copy. After up to 3 s it persists all tasks with partial sizes read from disk. Open connections then get up to 5 s to drain before the process exits, so SSE streams or large file transfers cannot block shutdown.

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

### Website (fyr.guide)
GitHub Pages deploys `docs/site/` verbatim, so generated site files are committed. `npm run build` also runs `docs/build/build-site.js`, which generates:
- `docs/site/css/site.css` — minified Tailwind CSS (pinned `tailwindcss` 3.4 + typography; config in `docs/build/site/tailwind.config.js`, source `docs/build/site/input.css`). There is no runtime CDN; fonts are self-hosted in `docs/site/fonts/`.
- `docs/site/guides/**`, `docs/site/compare/` — from HTML fragments with a `<!--meta {...} -->` header in `docs/build/site/content/`, wrapped in the shared layout (`docs/build/site/layout.js`).
- `docs/site/docs/user-manual/` and `docs/site/docs/developer-manual/` — rendered from the two canonical manuals (GitHub-compatible heading anchors).
- `docs/site/guides/index.html`, `docs/site/docs/index.html`, `docs/site/404.html`, `docs/site/sitemap.xml` (lastmod tracked by content hash in `docs/build/site/lastmod.json`).
- In `docs/site/index.html`, only the `<!-- build:… -->` regions (JSON-LD with `softwareVersion` from the workspace `Cargo.toml` and the FAQ, CTA band, footer, GitHub script) and the CSS cache-buster. Everything else on the landing page is hand-edited.

`npm run check` fails if any generated file is stale and runs `check-site.js` (one `<h1>`, heading order, unique title/description, canonical/OG tags, JSON-LD parsing, internal links and anchors, image alt/size, sitemap coverage, and `data-sync="index"` snippets matching the landing page). PR validation runs it, so edits to the manuals, `Cargo.toml` version, `index.html` FAQ or site sources need `npm run build` and a commit of the regenerated files. The deploy workflow (`.github/workflows/docs-site.yml`) rebuilds and checks before uploading.

Images: landing-page AVIF/WebP variants and `img/og-image.jpg` are generated from the source PNGs with `node optimize-images.js` after `npm install --no-save sharp@0.34.5` (not part of the regular build).

## 8. Road to 1.0 and Known Gaps

Fyr's 1.0 goal: a household, camp or community can run one box (Pi or homelab) as a hotspot and get maps for navigation, a searchable offline library, a local assistant grounded in that library, and local communication, with nothing that silently loses data or locks up the device.

### Delivered on `next` (1.0 candidates)
- **Library:** accent- and case-insensitive global search across books, ZIM article titles, Markdown/EPUB text, maps and POIs (§11); deep links, reading positions, bookmarks, "Continue reading", reader themes (§12).
- **Maps:** deep links, coordinate readout/copy/go-to (decimal, DMS, `geo:`), GPX import/export, track recording, line/polygon POIs.
- **Assistant:** "Ask your library" answers with cited sources (§1.1), context-window budgeting, prompt-injection hardening of library text, POST inference.
- **Messages:** public Board channel plus end-to-end encrypted passphrase channels (§3.5, rule 8 in AGENTS.md).
- **Content:** resumable, checksum-verified downloads; graceful shutdown that keeps partial downloads.
- **Platform:** installable web app manifest, lazy-loaded pages, `FYR_TRUSTED_PROXIES`, Vitest frontend tests, faster fyr.guide with guides and rendered manuals.

### Remaining before 1.0 (priority order)
1. **HTTPS without a separate proxy.** Browsers only allow geolocation (My Location, track recording) on HTTPS or localhost, and encrypted channels only resist active attackers over HTTPS. Offer built-in TLS (`FYR_TLS_CERT`/`FYR_TLS_KEY`, rustls) plus an installer option, or a documented Caddy recipe.
2. **Real-hardware validation.** Measure on a Pi 4/5 with real data: full Wikipedia ZIM search and retrieval latency, vector PMTiles/MBTiles styling, scrypt timing on low-end phones, memory use with a loaded model. The test suites run only on synthetic data.
3. **Full-text recall for ZIM.** Search and retrieval only match article titles in large archives. Use the Xapian full-text index embedded in many ZIM files (or build a compact one), shared by `search.rs` and `ai/retrieval.rs`.
4. **Place search on maps** from vector-tile place labels or bundled gazetteers, not only POI files.
5. **Internationalization** (at least Swedish): the UI is English-only today.
6. **PDF via pdf.js** for page memory, bookmarks and in-book search parity with EPUB.
7. **Backup and portability:** export/import of POI files, settings and (encrypted) channel keys; optional server-side reading progress for guests on shared devices.
8. **Curated catalog checksums:** fill `sha256` for curated downloads so one-click downloads are verified.
9. **Release engineering:** verified multi-arch Docker builds of `next`, an upgrade guide from 0.5.x (new `messages.sqlite`, new env vars), version bump to 1.0.0.

### Known gaps
- Interrupted URL downloads are not resumed automatically after a restart; the user clicks **Resume** (`POST /api/download/:task_id/retry`). Interrupted local imports are not resumable and must be re-imported.
- In open mode anyone on the network can post Board alerts (shown on Home); rate limits and admin deletion are the only mitigation. Use `FYR_ADMIN_PASSWORD` without `FYR_GUEST_MESSAGING` where that matters.
- Reading positions and bookmarks are per browser; there is no cross-device sync.
- CI checks for markdown/manual consistency are still basic and do not enforce cross-document semantic consistency.

## 9. Recommended Validation Sequence
Run from repository root unless noted:

1. `cargo test --workspace --all-targets`
2. `cargo check -p server`
3. `cd crates/ui/frontend && npm ci && npm run build`
4. `cd docs/build && npm ci && npm run build`
5. Validate native ZIM flow by opening a `.zim` file in Books and confirming article payload retrieval.

## 10. Native ZIM Reader Notes
1. Keep server-side reader contracts stable:
  - `/api/reader/zim/:filename/meta`
  - `/api/reader/zim/:filename/capabilities`
  - `/api/reader/zim/:filename/native/article`
  - `/api/reader/zim/:filename/native/content/*path`
2. Maintain clean-room implementation boundaries: all ZIM reader logic must be implemented directly in Fyr's Rust server code using the `zim` crate. Do not bundle or wrap third-party reader JavaScript bundles.
3. Validate representative archives after reader changes and monitor unsupported compression/edge-case failures.

## 11. Global Search (`/api/search`)
Implemented in `crates/server/src/search.rs`, routed publicly in `main.rs`; frontend in `pages/Search.vue`, `components/HeaderSearch.vue` and `modules/search/searchResults.js`.

**Request:** `GET /api/search?q=<text>&limit=<1..200, default 40>&kinds=<comma list>`. `kinds` accepts `book`, `article`, `content`, `map`, `poi` (unknown names are ignored; omitted = all). Blank `q` returns an empty result set; `q` is truncated to 200 characters.

**Response:** `{ query, results: [{ kind, title, subtitle?, source, score, link, lat?, lng? }], total, truncated, partial, elapsed_ms }`. `source` is a filename. `partial` means the time budget ran out before every source was searched.

**Link contracts** (consumed by Books/Maps; values are percent-encoded):
- book: `/books?file=<filename>`; ZIM article: `/books?file=<filename>&path=<zim path>` (same path form as `/native/search`); text hit: `/books?file=<filename>&q=<query>`
- map: `/maps?map=<filename>`; POI: `/maps?lat=<lat>&lng=<lng>&zoom=14&label=<name>` (no map param; Maps picks a covering map)

**Sources** (cheapest first): map filenames → book titles (`library::extract_title`, cached per path+size+mtime; PDFs > 16 MB use the filename) → GeoJSON POIs (FeatureCollection/Feature/array; Point or mean of line/polygon positions; parsed records cached; files > 32 MB skipped, ≤ 150k records per file) → ZIM titles → Markdown/EPUB full text via `library::search_book` (queries ≥ 3 chars, ≤ 40 books, size caps 8 MB / 32 MB).

**ZIM titles:** per archive, `article_list_by_title` (v1) or `entry_list_by_title` (v0) is used.
- ≤ 60k entries: a folded-title index is built synchronously and cached.
- ≤ 250k entries: the index is built on one background thread (one build at a time); meanwhile prefix lookup is used.
- Larger: prefix lookup only — binary search on the title-ordered listing for several case variants of the query (`as typed`, `Capitalised`, `Title Case`, lower, UPPER, underscores), walking forward while titles match. Cost is O(variants × (log n + limit)) directory reads regardless of archive size.
- No title listing: URL-order scan bounded by the deadline.
Redirects resolve to their target; duplicates by resolved path are dropped; at most 15 hits per archive. Index cache is LRU-bounded to 500k entries total (~20 MB).

**Ranking (pure, unit-tested):** case- and accent-folded (NFKD with combining marks removed, Unicode lowercase, `_` → space) tiers: exact 1000 > prefix 800 > word-start 600 > all tokens at word starts 450 > substring 300 (2+ chars) > text/description hit 250, minus ≤ 100 for extra length; books/maps +40, POIs +20. `merge_results` de-duplicates by `link` (best score wins), reserves 5 slots per kind, then fills by score.

**Bounds:** whole search runs in one `run_blocking` task with a 1.5 s budget (each ZIM archive gets an equal share of what remains, minimum 150 ms); at most 2 searches run concurrently (others queue on a semaphore); a dropped request (client abort) sets a cancel flag that stops the blocking work at its next check. Archive/parsing panics are caught per source.

**Testing:** pure ranking/merge/limit/link/cache tests and temp-file source tests live in `search.rs`; a router test in `main.rs` covers kinds, links, limits and path leakage in read-only mode. There is no ZIM writer, so ZIM paths are tested by `zim_title_search_against_fixture_archives`, which runs only when `FYR_TEST_ZIM_DIR` points at a folder of `.zim` files (for example the openZIM testing-suite `data/` archives). Frontend logic is tested in `modules/search/searchResults.test.js`.

## 12. Library Reading State (progress, bookmarks, deep links)
All of this lives in the browser. There are no server endpoints or server writes, so guests in password mode and in `FYR_READONLY` mode get the same features.

Modules (`crates/ui/frontend/src/modules/`):
- `library/deepLink.js`: pure mapping between `route.query` and reader state. Other pages should use `buildBookDeepLink(file, { path, q })` or `bookRoute(...)` rather than building URLs by hand. The contract is:
  - `file`: a library filename. Values containing `/`, `\`, `..` or control characters are rejected.
  - `path`: a ZIM article path. A `#fragment` is allowed and leading slashes are stripped.
  - `q`: an in-book search query.
- `library/progress.js`: `createProgressStore()` / `progressStore`, stored under the localStorage key `fyr.library.progress.v1`:
  - Stores the last-opened time, the position and the bookmarks for each book.
  - Keeps the 200 most recent books and 100 bookmarks per book.
  - Entries are held in an array rather than an object keyed by filename, so filenames such as `__proto__` cannot collide with prototype keys.
  - Every storage access is wrapped in try/catch (see `library/safeStorage.js`). If stored JSON is corrupt or has the wrong shape, it is treated as empty.
  - If storage is missing or throws, the store falls back to an in-memory copy. If a write fails because the quota is full, the store drops the older half of the books and retries once.
- Position shapes: `{type:'epub', cfi, href, label}`, `{type:'zim', path, label}`, `{type:'md', line, label}`. A `{type:'pdf', page}` shape is reserved; it is unused because the PDF iframe has no page API.
- `library/readerPrefs.js`: theme (`light`/`sepia`/`dark`) and font scale (70–200%), stored under `fyr.reader.prefs.v1`.
- `library/continueReading.js`: intersects `progressStore.listRecent()` with `GET /api/content/books` for the Home page.
- `reader/articleHistory.js`, `reader/shortcuts.js`, `reader/markdownPosition.js`: pure helpers. `shortcutForEvent` ignores inputs, textareas, selects, contenteditable elements and events with modifier keys.

Reader behaviour:
- `useUnifiedReader.selectBook()` creates a cancellation token for each selection. Readers check `token.cancelled` after every `await`, so a slow earlier book, or its error, never replaces the book the user switched to. `useZimReader.loadNativeArticle()` numbers its requests in sequence and returns `null` for a superseded request.
- EPUB positions come from epub.js `relocated` events (saved after an 800 ms debounce) and are restored with `rendition.display(cfi)`. If the CFI is invalid, the reader falls back to the start of the book.
- Markdown is rendered block by block. `renderMarkdownWithLines` wraps each top-level block in `data-md-line="<source line>"`. Positions use the top visible block, and `/api/library/books/:f/search` hits (`#<line>`) scroll to the block that contains that line.
- ZIM positions are the current article path (including any `#hash`). If the start path fails to load, the reader shows the main page and displays a notice.
- `Books.vue` mirrors the reader state to the URL with `router.replace`. It records the queries it set itself, so its route watcher can tell its own URL updates apart from external navigation (for example a `/books` nav click, which closes the book). If the requested file is not in the library listing, the page shows a notice and clears the query.
- Pending saves are written immediately on book switch, on `pagehide`, when the page becomes hidden, and on unmount.

Tests: `cd crates/ui/frontend && npm test` runs Vitest with the shared happy-dom environment. Tests live next to the modules in `*.test.js` files. `readerHelpers.test.js` replaces DOMPurify with an identity function, because happy-dom's parser makes DOMPurify unwrap the first top-level element of each fragment; real browsers are not affected. That file tests line anchoring, not sanitization.
