# Fyr User Manual

## 1. What Fyr Is
Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local server and is accessed from a browser.

## 2. Start Fyr
For complete installation instructions—including building from source, running via Docker, or setting up a Raspberry Pi—please refer to the authoritative [INSTALL.md](/INSTALL.md).

Once Fyr is running, open `http://localhost:8080` on the same machine, or `http://<host-or-device-ip>:8080` if Fyr runs in Docker or on another device.

---

## 3. Main Pages
- **Home:** system status, location, sunrise/sunset, and storage overview.
- **Content Manager:** add URL downloads, import local files, and inspect content inventory. Requires admin access when `FYR_ADMIN_PASSWORD` is set.
- **Settings:** configure location and other application-wide preferences. Requires admin access when `FYR_ADMIN_PASSWORD` is set; hidden in read-only mode.
- **Maps:** map selection and viewer controls.
- **Books:** browse books, read EPUB/PDF/Markdown, and launch ZIM reader flow.
- **Assistant:** browse local `.gguf` models and chat offline.

## 3b. Access Control and Admin Login

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

## 3a. Using the AI Assistant
- Open the Assistant tab from the top navigation.
- Use **Open Content Manager** from the Assistant sidebar to jump to the Models section for `.gguf` uploads.
- For text generation, use GGUF files that include tokenizer metadata.
- Select a model and press **Load Model**.
- Enter a prompt and send it to start token streaming.

> **Model choice notes:**
> * Larger models and higher quantization levels use more memory.
> * If responses are slow, try smaller quantized variants (for example Q4 instead of Q8).
> * Fyr's inference runtime currently supports the **Qwen2** model family for text generation. The curated defaults focus on `Qwen2.5-1.5B`, `Qwen2.5-3B`, `Qwen2.5-7B`, and `Qwen2.5-14B` GGUF builds.
> * Models with a built-in reasoning mode (such as Qwen3 or DeepSeek-R1) emit a `<think>…</think>` block before their response. Fyr displays this reasoning in a collapsible **Thinking** section above the response — it streams live while the model reasons and collapses automatically when reasoning is complete.

### Where to find compatible models

Fyr ships a manually editable curated catalog at `public/data/curated-content.json` (served at `/data/curated-content.json`). It records tested model tiers together with recommended download sources for books and maps, and can also include optional direct `download_url` entries for one-click downloads in Content Manager.

Recommended model tiers from that catalog:

- **Small** — `Qwen2.5-1.5B-Instruct` in `Q8_0` (~1.7 GB) for Raspberry Pi 5 systems with 4 GB RAM or for faster/simple answers.
- **Standard / Recommended** — `Qwen2.5-3B-Instruct` in `Q6_K` (~2.6 GB) for the best balance on Raspberry Pi 5.
- **Large** — `Qwen2.5-7B-Instruct` in `Q4_K_M` (~4.5 GB) for Raspberry Pi 5 systems with 8 GB RAM.
- **Extra large / Desktop** — `Qwen2.5-14B-Instruct` in `Q4_K_M` (~9.8 GB), or `Qwen2.5-7B-Instruct` in `Q8_0` (~8.5 GB), for 16 GB+ systems.

GGUF files can be downloaded from [Hugging Face](https://huggingface.co/models?library=gguf&sort=trending). Recommended search:

- Search: `Qwen2.5 GGUF` — filter by library `GGUF`
- Well-known publisher: **Qwen** org (`Qwen/Qwen2.5-1.5B-Instruct-GGUF`, `Qwen/Qwen2.5-3B-Instruct-GGUF`, `Qwen/Qwen2.5-7B-Instruct-GGUF`, `Qwen/Qwen2.5-14B-Instruct-GGUF`)
- Fyr's **Balanced** mode uses `temperature=0.2` and `max_tokens=512`; the **Precise** mode uses `temperature=0.1`; the **Creative** mode uses `temperature=0.7` and `max_tokens=1024`
- Fyr defaults to a `num_ctx` of `2048`; on systems with more than 16 GB of RAM it automatically expands to `8192`
- Advanced users can force the larger context window by setting `settings.modules.assistant.high_ram_context` to `true`

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
- Put `.pmtiles` files in `public/data/maps/`.
- A practical source is OpenStreetMap-derived extracts packaged as PMTiles from trusted providers.

### POI
- Put `.geojson`, `.fgb`, or `.json` POI datasets in `public/data/poi/`.
- If using `.json`, make sure it follows a valid geo dataset structure used by your workflow.

### Models
- Open **Content Manager** and upload a `.gguf` file in the Models section.
- Fyr validates the GGUF header, stores the upload in `public/data/inbox/`, then imports it into `public/data/models/`.
- Current inference runtime is implemented for GGUF models with `qwen2` architecture.
- Other GGUF architectures can still be loaded for validation/health checks but may not support text generation yet.
- Prefer models that include tokenizer metadata in GGUF.
- The bundled curated catalog (`public/data/curated-content.json`) lists the recommended Qwen 2.5 GGUF tiers and their default RAG settings.

### Misc
- Use `public/data/misc/` for generic files that are not map/book/poi/model types.
- Good examples: offline installers, utility archives, drivers, checksum lists, and operational notes.

### Downloads
- Use **Content Manager** to queue URL downloads.
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
- Use the search input above the article panel to find entries by title or path, then open results directly in the same reader view.
- Links inside articles are handled by the embedded reader shell and load new native article views without leaving the Books page.
- Fyr fetches archive metadata and article content through local `/api/reader/zim/*` endpoints.
- Book archives remain available under `/docs/books/<filename>.zim` for local access.

## 5c. Reader Shell
- Books uses a unified reader shell with format badges and open/loading/error status badges.
- EPUB, Markdown, PDF, and ZIM open in the same reader area, while format-specific controls (like ZIM search) appear only when relevant.
- On narrow screens, the library list stacks above the reader panel automatically.

## 5a. Markdown Reading
- Select a `.md` file in Books to open it in the built-in markdown reader.
- Markdown manuals are distributed as regular `.md` files in `public/data/books/`.

## 5b. PDF Reading
- Select a `.pdf` file in Books to open it inline in the built-in reader panel.
- If your browser blocks inline PDF rendering, use the "open it in a new tab" link shown under the reader panel.

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
- If the model still loads but will not generate text, check the assistant status line for tokenizer or runtime errors.

### Assistant load fails or runs slowly
- Check model health in the assistant status line.
- If memory is limited, use a smaller quantized model.
- If tokenizer metadata is missing, re-export the model with tokenizer fields included.
