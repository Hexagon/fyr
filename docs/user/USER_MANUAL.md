# Fyr User Manual

## 1. What Fyr Is
Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local server and is accessed from a browser.

## 2. Start Fyr
### Binary
1. Build or download the `fyr` binary.
2. Run `./target/release/fyr` (or `fyr.exe` on Windows).
3. Open `http://127.0.0.1:8080`.

### Docker
Run Fyr in a container using either production or dev image tags.

Production release:

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

Dev release:

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:dev
```

Then open `http://127.0.0.1:8080`.

## 3. Main Pages
- Home: system status, location, sunrise/sunset, and storage overview.
- Content Manager: add downloads, upload `.gguf` models, and inspect content inventory.
- Maps: map selection and viewer controls.
- Books: browse books, read EPUB, launch ZIM reader flow.
- Assistant: browse local `.gguf` models and chat offline.

Header behavior:
- The top header shows the current page context together with the clock, weekday, and date.
- Location details, sunrise/sunset, server status, and version are shown in the Overview status card instead of the header.

## 3a. Using the AI Assistant
- Open the Assistant tab from the top navigation.
- Use **Import Model** to upload a local `.gguf` file. Fyr stores it in `public/data/inbox/` and imports it into the model library automatically.
- For text generation, use GGUF files that include tokenizer metadata.
- Select a model and press **Load Model**.
- Enter a prompt and send it to start token streaming.

Model choice notes:
- Larger models and higher quantization levels use more memory.
- If responses are slow, try smaller quantized variants (for example Q4 instead of Q8).

## 4. Add Content
### Data directories and supported file types
All data is stored under `public/data/` (or `DATA_DIR` if you override it).

| Folder | Supported file types | Typical use |
|---|---|---|
| `books/` | `.epub`, `.pdf`, `.mobi`, `.md`, `.zim` | Offline books, manuals, and archives |
| `maps/` | `.pmtiles` | Offline map tiles |
| `poi/` | `.geojson`, `.fgb`, `.json` | POI layers and geo datasets |
| `models/` | `.gguf` | Local AI models for Assistant |
| `misc/` | `.txt`, `.csv`, `.zip`, `.7z`, `.log`, installer packages such as `.exe`, `.msi`, `.deb`, `.rpm`, `.dmg`, `.pkg` | General offline resources and installers |
| `inbox/` | temporary `.gguf` during import | Staging area used by model upload flow |

### Books
- Put local book files in `public/data/books/`.
- URL downloads with supported book extensions are routed to `books/` automatically.
- For ZIM archives, use Kiwix's library and download index:
  - `https://library.kiwix.org`
  - `https://download.kiwix.org/zim/`

### Maps
- Put `.pmtiles` files in `public/data/maps/`.
- A practical source is OpenStreetMap-derived extracts packaged as PMTiles from trusted providers.

### POI
- Put `.geojson`, `.fgb`, or `.json` POI datasets in `public/data/poi/`.
- If using `.json`, make sure it follows a valid geo dataset structure used by your workflow.

### Models
- Open **Assistant** or **Content Manager** and upload a `.gguf` file.
- Fyr validates the GGUF header, stores the upload in `public/data/inbox/`, then imports it into `public/data/models/`.
- Current inference runtime is implemented for GGUF models with `qwen2` architecture.
- Other GGUF architectures can still be loaded for validation/health checks but may not support text generation yet.
- Prefer models that include tokenizer metadata in GGUF.

### Misc
- Use `public/data/misc/` for generic files that are not map/book/poi/model types.
- Good examples: offline installers, utility archives, drivers, checksum lists, and operational notes.

### Downloads
- Use **Content Manager** to queue URL downloads.
- Downloads are auto-routed by recognized extension to the correct folder.
- If a URL points to an unrecognized extension, the file remains in `inbox/` until you move it manually.
- Active tasks persist across restarts and are restored automatically.
- You can cancel queued or in-progress downloads from the downloads panel.

## 5. ZIM Reading
- Select a `.zim` file in Books and Fyr opens it directly in the embedded reader.
- Kiwix web bundle is served from `/kiwix/www/index.html`.
- Book archives are served at `/docs/books/<filename>.zim` with byte-range support.
- Capabilities endpoint: `/api/reader/kiwix/capabilities`.

Licensing note for embedded reader:
- Fyr core project code is MIT-licensed.
- The embedded Kiwix bundle under `public/kiwix-static/` is a third-party component with its own copyleft licenses.
- License texts are distributed in:
  - `public/kiwix-static/LICENSE-GPLv3.txt`
  - `public/kiwix-static/LICENSE-AGPLv3.txt`
  - `public/kiwix-static/THIRD_PARTY_NOTICES.txt`

## 5a. Markdown Reading
- Select a `.md` file in Books to open it in the built-in markdown reader.
- Markdown manuals are distributed as regular `.md` files in `public/data/books/`.
- In Docker setups with persistent `DATA_DIR`, Fyr refreshes `user-manual.md` and `developer-manual.md` automatically at startup.

## 6. Data Storage Layout
`public/data/` is created automatically:
- `public/data/maps/`
- `public/data/books/`
- `public/data/poi/`
- `public/data/inbox/`
- `public/data/models/`
- `public/data/misc/`

Environment overrides:
- `DATA_DIR`
- `FYR_HOST`
- `FYR_PORT`

## 7. Platform Notes
- Intel/AMD (`x86_64`) and ARM64 (`aarch64`) are both supported.
- Raspberry Pi works best with a 64-bit OS and ARM64 build/image.
- For access from other devices on the LAN, run with `FYR_HOST=0.0.0.0`.

## 8. Common Troubleshooting
### Server does not start
- Check if port `8080` is already in use.
- Stop old `fyr` processes and retry.
- If startup reports bind failure details, verify `FYR_HOST` and `FYR_PORT` values.
- If startup reports write permission issues, ensure `DATA_DIR` points to a writable folder.

### Content not visible in UI
- Verify file extension is supported.
- Verify file is in the correct `data/` subfolder.
- Refresh browser after server restart.

### Download is stuck or failed
- Open Content Manager and inspect the download status/error line.
- Cancel the task and retry the URL.
- For repeated failures, verify the source URL is reachable and supports direct file transfer.

### Kiwix view not loading
- Confirm `public/kiwix-static/www/index.html` exists.
- Check `/api/kiwix/status`.

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
