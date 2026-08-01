# Fyr User Manual

## 1. What Fyr Is
Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local server and is accessed from a browser.

> **Project status:** Fyr is currently in **preview**. UI details may evolve as features are refined.

### Related documentation
- Installation and deployment paths: [fyr.guide/#installation](https://fyr.guide/#installation)
- Project overview: [README.md](../../README.md)
- Developer architecture details: [Developer Manual](../developer/DEVELOPER_MANUAL.md)

### 1a. Installer Scripts

Official hosted scripts are available for a guided Docker setup:

| Platform | One-liner |
|----------|-----------|
| Linux / macOS | `curl -fsSL https://fyr.guide/install.sh | sh` |
| Windows PowerShell | `irm https://fyr.guide/install.ps1 | iex` |

The scripts persist settings (port, data volume, admin password) in `~/.config/fyr/install.conf` (Linux/macOS) or `%APPDATA%\fyr\install.conf` (Windows). To upgrade to a newer image, add `update`:

```bash
curl -fsSL https://fyr.guide/install.sh | sh -s -- update
```

```powershell
irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Update
```

All arguments are documented inline via `--help` (Linux/macOS) or `-Help` (Windows), and the full reference table is available at [fyr.guide/#installation](https://fyr.guide/#installation).

## 2. Start Fyr
For complete installation instructions—including building from source, running via Docker, or setting up a Raspberry Pi—use [fyr.guide/#installation](https://fyr.guide/#installation). (If viewing this file locally, open `docs/site/index.html` in a browser for the same installation guide.)

Once Fyr is running, open `http://localhost:8080` on the same machine, or `http://<host-or-device-ip>:8080` if Fyr runs in Docker or on another device.

---

## 3. Main Pages
- **Home:** system status, location, sunrise/sunset, and storage overview.
- **Content Manager:** add URL downloads, import local files, and inspect content inventory. Requires admin access when `FYR_ADMIN_PASSWORD` is set.
- **Settings:** configure location and other application-wide preferences. Requires admin access when `FYR_ADMIN_PASSWORD` is set; hidden in read-only mode.
- **Maps:** map selection and viewer controls.
- **Books:** browse books, read EPUB/PDF/Markdown/ZIM in a unified reader shell, and use auto-collapse Library focus mode when opening a title.
- **Assistant:** browse local `.gguf` models and chat offline.
- **Tools:** unit converters (length, mass, temperature, area, volume, speed, data, angle, pressure, energy, power, time) and encryption/ciphering utilities (AES-256-CBC, Base64, ROT13, SHA-256, MD5). All operations are local and offline-safe—no server communication or admin access required.

## 3a. Access Control and Admin Login

Fyr can run in three access modes:

### Open mode (default)
All features are available to everyone. No authentication is needed.

### Password-protected mode (`FYR_ADMIN_PASSWORD`)
The server operator sets an admin password via the `FYR_ADMIN_PASSWORD` environment variable.

**What guests can do:**
- Browse maps, books, and POIs
- Chat with pre-loaded AI models
- Read content

**What requires admin login:**
- Content Manager (downloads, imports, file deletion)
- Settings (location and preferences)
- Model uploads and model management
- Storage details on the Overview page

**How to log in:**
1. Click **Log in** in the top-right navbar.
2. Enter the admin password on the login page.
3. On success, an **Admin** badge appears in the navbar alongside a **Log out** button.
4. The Content Manager, Settings nav links and storage details become visible.

**How to log out:**
Click **Log out** in the navbar. Your session is cleared immediately.

**Security notes:**
- Session tokens are stored server-side in memory; the HttpOnly cookie contains only the token reference (not the session data itself). Cookies are not accessible to JavaScript.
- Failed login attempts are rate-limited per IP address (10 attempts per 5 minutes, using the real TCP peer address).
- If you forget the password, restart the server with the correct `FYR_ADMIN_PASSWORD` value.

### Strict read-only mode (`FYR_READONLY`)
Setting `FYR_READONLY=true` disables all mutating operations permanently. No login is possible. The Content Manager, Settings, and model management links are hidden. The server shows a **Read-only** badge in the navbar.

Use this for kiosk or public library deployments where content is pre-loaded and no runtime management is needed.

### Header behavior
- The top header shows the current page context together with the clock, weekday, and date.
- Location details, sunrise/sunset, server status, and version are shown in the Overview status card instead of the header.

## 3b. Using the AI Assistant
- Open the Assistant tab from the top navigation.
- Use **Open Content Manager** from the Assistant sidebar to jump to the Models section for `.gguf` uploads.
- For text generation, use GGUF files that include tokenizer metadata.
- Select a model; Fyr loads it automatically when possible.
- Enter a prompt and send it to start live token streaming.

> **Model choice notes:**
> * Larger models and higher quantization levels use more memory.
> * If responses are slow, try smaller quantized variants (for example Q4 instead of Q8).
> * Fyr's inference runtime currently supports GGUF models with **Qwen2**, **Llama**, and **Phi-3/Phi-3.5** architectures.
> * The assistant shows a **Thinking** block immediately after you send a prompt, then streams the visible reply as it arrives.
> * Models with a built-in reasoning mode (such as Qwen3 or DeepSeek-R1) emit a `<think>…</think>` block before their response. Fyr displays that reasoning in the same collapsible **Thinking** section and streams it live while the model reasons.

### Where to find compatible models

Fyr ships a manually editable curated catalog at `public/data/curated-content.json` (served at `/data/curated-content.json`). It records tested model tiers together with recommended download sources for books and maps, and can also include optional direct `download_url` entries for one-click downloads in Content Manager.

Recommended model tiers from that catalog:

- **Small** — `Qwen2.5-1.5B-Instruct` in `Q8_0` (~1.7 GB) for Raspberry Pi 5 systems with 4 GB RAM or for faster/simple answers.
- **Standard / Recommended** — `Qwen2.5-3B-Instruct` in `Q6_K` (~2.6 GB) for the best balance on Raspberry Pi 5.
- **Large** — `Qwen2.5-7B-Instruct` in `Q4_K_M` (~4.5 GB) for Raspberry Pi 5 systems with 8 GB RAM.
- **Extra large / Desktop** — `Qwen2.5-14B-Instruct` in `Q4_K_M` (~9.8 GB), or `Qwen2.5-7B-Instruct` in `Q8_0` (~8.5 GB), for 16 GB+ systems.
- **Llama alternative** — `Llama-3.2-3B-Instruct` in `Q4_K_M` (~2.0 GB) when you want a broadly compatible multilingual instruct model.
- **Phi alternative** — `Phi-3.5-mini-instruct` in `Q4_K_M` (~2.4 GB) when you want a compact reasoning-oriented model.

GGUF files can be downloaded from [Hugging Face](https://huggingface.co/models?library=gguf&sort=trending). Recommended search:

- Search: `Qwen2.5 GGUF` — filter by library `GGUF`
- Well-known publisher: **Qwen** org (`Qwen/Qwen2.5-1.5B-Instruct-GGUF`, `Qwen/Qwen2.5-3B-Instruct-GGUF`, `Qwen/Qwen2.5-7B-Instruct-GGUF`, `Qwen/Qwen2.5-14B-Instruct-GGUF`)
- Search: `Llama 3.2 3B Instruct GGUF` — a common mirror is `bartowski/Llama-3.2-3B-Instruct-GGUF`
- Search: `Phi-3.5 mini instruct GGUF` — common sources are `bartowski/Phi-3.5-mini-instruct-GGUF` and Microsoft's official `Phi-3` GGUF repositories
- Fyr's **Balanced** mode uses `temperature=0.2` and `max_tokens=512`; the **Precise** mode uses `temperature=0.1`; the **Creative** mode uses `temperature=0.7` and `max_tokens=1024`
- Fyr defaults to a `num_ctx` of `2048`; on systems with more than 16 GB of RAM it automatically expands to `8192`
- Advanced users can force the larger context window by setting `settings.modules.assistant.high_ram_context` to `true`
- Some Llama-family downloads are gated by Hugging Face license acceptance. In those cases, Content Manager may link you to the source page rather than providing a one-click direct download.

Once downloaded, upload the `.gguf` file through Content Manager → Models.

### Conversation context and modes

The Assistant keeps track of recent conversation turns and sends the last six messages as context when inferring, so the model can reference what was discussed earlier in the session.

Three response modes are available:

| Mode | Behaviour |
|------|-----------|
| **Precise** | temperature=0.1, max_tokens=512 — focused, factual answers |
| **Balanced** | temperature=0.2, max_tokens=512 — default, concise and reliable |
| **Creative** | temperature=0.7, max_tokens=1024 — more elaborate, varied responses |

### Persisting the default model

The last model you selected is remembered in browser storage. When you re-open the Assistant, Fyr will automatically re-select and attempt to load that model. If the load fails (e.g. the model file was removed), an error message is shown in the chat and you can select another model manually.

## 4. Add Content
### Data directories and supported file types
All data is stored under `public/data/` (or `DATA_DIR` if you override it).

| Folder | Supported file types | Typical use |
| --- | --- | --- |
| `curated-content.json` | structured JSON catalog | Manually editable list of recommended model, book, and map downloads |
| `books/` | `.epub`, `.pdf`, `.mobi`, `.md`, `.zim` | Offline books, manuals, and archives |
| `maps/` | `.pmtiles` | Offline map tiles |
| `poi/` | `.geojson`, `.fgb`, `.json` | POI layers and geo datasets |
| `models/` | `.gguf` | Local AI models for Assistant |
| `misc/` | `.txt`, `.csv`, `.zip`, `.7z`, `.log`, `.exe`, `.msi`, `.deb`, `.rpm`, `.dmg`, `.pkg` | General offline resources and installers |
| `inbox/` | temporary files during upload/import | Staging area used by import workflows before auto-routing |

### Books
- Put local book files in `public/data/books/`.
- URL downloads with supported book extensions are routed to `books/` automatically.
- For ZIM archives, use trusted OpenZIM-compatible sources.

### Maps
- Fyr serves vector and raster map tiles from `.pmtiles` archives in `public/data/maps/`.
- PMTiles is a single-file archive format for map tiles, readable directly by the browser without a tile server.

**Obtaining PMTiles maps:**

**Pre-compiled Shortbread maps (BBBike):**
Download ready-to-use `.pmtiles` files from [data.bbbike.org](https://data.bbbike.org/osm/region/).

**Extract from Protomaps (CLI):**
Install the `pmtiles` CLI ([docs.protomaps.com/pmtiles/cli](https://docs.protomaps.com/pmtiles/cli)) and extract a region from a global PMTiles archive:

```
pmtiles extract https://build.protomaps.com/20260716.pmtiles sweden.pmtiles --bbox=4.7,55.0,24.2,69.1 --maxzoom=15
pmtiles extract https://build.protomaps.com/20260716.pmtiles world.pmtiles --bbox=-180,-85.0511,180,85.0511 --maxzoom=8
```

### POI
- Put `.geojson`, `.fgb`, or `.json` POI datasets in `public/data/poi/`.
- If using `.json`, make sure it follows a valid geo dataset structure used by your workflow.

### Models
- Open **Content Manager** and upload a `.gguf` file in the Models section.
- Fyr validates the GGUF header, stores the upload in `public/data/inbox/`, then imports it into `public/data/models/`.
- Current inference runtime is implemented for GGUF models with `qwen2`, `llama`, and `phi3` architectures.
- Prefer models that include tokenizer metadata in GGUF.
- For `phi3`/`phi-3.5` models, keep a tokenizer sidecar in the same folder (preferred: `public/data/models/Phi-3.5-mini-instruct-Q4_K_M.tokenizer.json`; fallback names: `tokenizer.json` or `<model>.json`).
- The bundled curated catalog (`public/data/curated-content.json`) lists recommended Qwen2.5, Llama 3.2, and Phi-3.5 GGUF tiers together with their default RAG settings.

### Misc
- Use `public/data/misc/` for generic files that are not map/book/poi/model types.
- Good examples: offline installers, utility archives, drivers, checksum lists, and operational notes.

### Downloads
- Use **Content Manager** to queue URL downloads.
- Large URL download timeout is centrally configurable from **Settings → Downloads** as **Request timeout (seconds)**.
- Advanced path: the same value is persisted in `settings.modules.downloads.request_timeout_seconds`.
- When a content folder is empty, Content Manager shows curated recommendations from `curated-content.json` instead of a blank listing.
- When a content folder already has files, Content Manager keeps those recommendations visible as suggested additional sources.
- Use the **Local Imports** panel in Content Manager (button or drag/drop) to upload local files and enqueue local import tasks.
- Use the **Download** button in each Content Manager file row to download a local copy from the browser.
- Downloads are auto-routed by recognized extension to the correct folder.
- If a URL points to an unrecognized extension, the file remains in `inbox/` until you move it manually.
- Active tasks persist across restarts and are restored automatically.
- Content listings and download tasks refresh automatically as task state changes.
- You can cancel queued or in-progress downloads from the download manager.
- Fyr does not overwrite your `curated-content.json` catalog when it already exists in `DATA_DIR`, so you can keep local recommendations there across updates.

## 5. ZIM Reading
- Select a `.zim` file in Books and Fyr opens it using the native reader module.
- Use the search input in the top reader toolbar to find entries by title or path, then open results directly in the same reader view.
- Links inside articles are handled by the embedded reader shell and load new native article views without leaving the Books page.
- ZIM pages use archive-provided styles and default browser styles only; Fyr does not inject fallback theme styles.
- Fyr fetches archive metadata and article content through local `/api/reader/zim/*` endpoints.
- Book archives remain available under `/docs/books/<filename>.zim` for local access.

## 5a. Markdown Reading
- Select a `.md` file in Books to open it in the built-in markdown reader.
- Markdown manuals are distributed as regular `.md` files in `public/data/books/`.

## 5b. PDF Reading
- Select a `.pdf` file in Books to open it inline in the built-in reader panel.
- If your browser blocks inline PDF rendering, use the "open it in a new tab" link shown under the reader panel.

## 5c. Reader Shell
- Books uses a unified reader shell with a single top toolbar.
- The toolbar includes back-to-library, title + filename, format/status badges, and compact metadata chips.
- EPUB, Markdown, PDF, and ZIM open in the same reader area, while format-specific controls (like ZIM search) appear only when relevant.
- Opening any book auto-collapses and hides the Library panel so the reader gets maximum space; use the back button in the reader toolbar to return to the full Library list.
- Reader scrolling is owned by the active reader surface to avoid nested page/reader double-scroll behavior.

## 6. Data Storage Layout
`public/data/` is created automatically and contains the following directories:

- `public/data/maps/`
- `public/data/books/`
- `public/data/poi/`
- `public/data/inbox/`
- `public/data/models/`
- `public/data/misc/`


### System-Managed Manuals Sync

On startup, Fyr automatically refreshes the two system-managed manuals in `DATA_DIR/books/`:

* `user-manual.md`
* `developer-manual.md`

Other files under `DATA_DIR` are preserved as user-managed content.

### Environment overrides

- `DATA_DIR`
- `FYR_HOST`
- `FYR_PORT`
- `FYR_ADMIN_PASSWORD` — enables password-protected admin mode (see §3a)
- `FYR_READONLY` — enables strict read-only mode; all mutating endpoints return 403 (see §3a)
- `FYR_AI_THREADS` — optional override for the AI inference thread pool size (default: number of available CPU cores, respecting container CPU quotas)

`FYR_HOST` changes where the server listens. Keep `127.0.0.1` for local-only access, or use `0.0.0.0` when Fyr runs in Docker or should accept LAN traffic. In the browser, use the host machine's name or IP address together with `FYR_PORT`.

## 7. Platform Notes
- Intel/AMD (`x86_64`) and ARM64 (`aarch64`) are both supported.
- Raspberry Pi works best with a 64-bit OS and ARM64 build/image.
- For access from other devices on the LAN, run with `FYR_HOST=0.0.0.0`.

## 8. Common Troubleshooting
### Server does not start
- Check if port `8080` is already in use. Change `FYR_PORT` and host port mapping (for Docker) if the port is in use.
- Stop old `fyr` processes and retry.
- If startup reports bind failure details, verify `FYR_HOST` and `FYR_PORT` values.
- If startup reports write permission issues, ensure `DATA_DIR` points to a writable folder.

### Content not visible in UI
- Verify file extension is supported.
- Verify file is in the correct `data/` subfolder.
- Refresh browser after server restart.
- Missing content after a restart generally means you need to confirm the `/data` volume is mounted.

### Download is stuck or failed
- Open Content Manager and inspect the download status/error line.
- Cancel the task and retry the URL.
- For repeated failures, verify the source URL is reachable and supports direct file transfer.

### ZIM view not loading
- Confirm the selected `.zim` file exists under `public/data/books/` (or your configured `DATA_DIR/books/`).
- Check the server status and retry opening the archive.

### Assistant model import fails
- Confirm the model file extension is `.gguf`.
- Ensure the source file starts with GGUF magic bytes.
- Retry the upload if the browser was interrupted before the file finished transferring.

### Assistant inference fails after load
- Confirm the model architecture is currently supported by Fyr inference.
- Confirm the `.gguf` model includes tokenizer metadata.
- For `phi3`/`phi-3.5`, confirm a sidecar tokenizer JSON exists next to the `.gguf` model.
- If the model still loads but will not generate text, check the assistant status line for tokenizer or runtime errors.

### Assistant load fails or runs slowly
- Check model health in the assistant status line.
- If memory is limited, use a smaller quantized model.
- If tokenizer metadata is missing, re-export the model with tokenizer fields included.
- For `phi3`/`phi-3.5`, use the original `tokenizer.json` from the model repository and place it beside the model file.
- On constrained hardware (e.g. Raspberry Pi), check the server startup logs for CPU feature information. If the log shows `dotprod` is available at runtime but not compiled in, rebuild the Docker image with `--build-arg RUST_TARGET_FEATURES=+dotprod` for faster quantized inference. You can also adjust the inference thread pool by setting `FYR_AI_THREADS=<n>` (default: number of available CPU cores).

## 9. Tools

The **Tools** page (accessible from the top navigation bar) provides common offline utilities organized into two tabs. All operations run entirely in your browser—no data is sent to the server or over the network.

### Unit Converters

The **Unit Converters** tab supports twelve conversion categories, grouped into logical sections:

| Section | Category | Units |
|---------|----------|-------|
| Length & Speed | Length | mm, cm, m, km, in, ft, yd, mi |
| | Speed | m/s, km/h, mph, knot |
| Weight & Volume | Mass | mg, g, kg, oz, lb |
| | Volume | mL, L, m³, fl_oz, gal, cup |
| Area & Angle | Area | mm², cm², m², km², ha, in², ft², ac |
| | Angle | deg, rad, grad |
| Temperature | Temperature | C, F, K |
| Digital Storage | Data | B, KB, MB, GB, TB, KiB, MiB, GiB |
| Energy & Power | Energy | J, kJ, cal, kcal, Wh, kWh |
| | Power | W, kW, MW, HP, BTU/h |
| Pressure & Time | Pressure | Pa, kPa, MPa, bar, psi, atm, mmHg |
| | Time | ms, s, min, h, day |

**How to use a converter:** Enter a numeric value, choose the source unit and target unit from the dropdowns. The converted result updates immediately as you type or change selections.

### Encryption & Ciphers

The **Encryption & Ciphers** tab provides four tools:

- **AES-256-CBC:** Encrypt or decrypt text with a password using AES-256 in CBC mode. Encryption produces a hex-encoded string containing the salt, IV, and ciphertext. Decryption requires the same password used during encryption. Uses PBKDF2 with 100,000 iterations for key derivation.

- **Base64:** Encode plain text to Base64 or decode Base64 back to plain text. Handles Unicode text correctly.

- **ROT13:** Apply the classic ROT13 letter substitution cipher (A↔N, B↔O, etc.). Non-letter characters pass through unchanged. Applying ROT13 twice recovers the original text.

- **Hash / Checksum:** Compute cryptographic hashes of arbitrary text input. Four algorithms are supported: **SHA-256**, **SHA-512**, **SHA-1**, and **MD5**. The output is displayed as a lowercase hex string. Use this for verifying file checksums or generating content digests.

> **Algorithm implementation notes:**
> * **AES-256-CBC** uses the browser's Web Crypto API with PBKDF2 key derivation (SHA-256, 100,000 iterations) and a random IV per encryption. The hex output format is Fyr-specific and cannot be directly decrypted by standard tools without extracting the salt and IV.
> * **Base64** uses the browser's built-in `btoa`/`atob` with UTF-8 safe encoding via `encodeURIComponent`. Results match the standard Base64 alphabet.
> * **ROT13** applies the classic single-pass rotation; non-letter characters are unaffected.
> * **SHA-256, SHA-512, and SHA-1** use the browser's Web Crypto digest API and produce standard lowercase hex digests identical to `sha256sum`, `sha512sum`, and `sha1sum` command-line tools.
> * **MD5** uses a self-contained JavaScript implementation that produces standard lowercase hex digests. It has been verified against the reference RFC 1321 test vectors and matches the output of `md5sum`.
>
> **Security note:** The AES tool is designed for convenience and casual use. For high-security applications, use purpose-built encryption tools with audited key management. MD5 and SHA-1 are cryptographically broken and should not be used for security purposes; they are included for legacy checksum verification.
