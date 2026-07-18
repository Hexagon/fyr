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
Run Fyr in a container using the standard image name:

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

Then open `http://127.0.0.1:8080`.

## 3. Main Pages
- Home: system status and storage overview.
- Content Manager: add downloads and inspect content inventory.
- Maps: map selection and viewer controls.
- Books: browse books, read EPUB, launch ZIM reader flow.
- Assistant: browse local `.gguf` models and chat offline.

## 3a. Using the AI Assistant
- Open the Assistant tab from the top navigation.
- Import models by placing `.gguf` files in `public/data/inbox/` or `public/data/misc/`, then using the assistant import action.
- Select a model and press **Load Model**.
- Enter a prompt and send it to start token streaming.

Model choice notes:
- Larger models and higher quantization levels use more memory.
- If responses are slow, try smaller quantized variants (for example Q4 instead of Q8).

## 4. Add Content
### Books
Place files in `public/data/books/` (or the folder set with `DATA_DIR`):
- `.epub`
- `.pdf`
- `.mobi`
- `.zim`

### Maps
Place `.pmtiles` files in `public/data/maps/`.

### POI
Place `.geojson` or `.fgb` files in `public/data/poi/`.

## 5. ZIM Reading
- Select a `.zim` file in Books and Fyr opens it directly in the embedded reader.
- Kiwix web bundle is served from `/kiwix/www/index.html`.
- Book archives are served at `/docs/books/<filename>.zim` with byte-range support.
- Capabilities endpoint: `/api/reader/kiwix/capabilities`.
- Server-side ZIM helper endpoints are available:
  - `/api/zim/:filename/meta`
  - `/api/zim/:filename/main`
  - `/api/zim/:filename/content/*path`

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

### Content not visible in UI
- Verify file extension is supported.
- Verify file is in the correct `data/` subfolder.
- Refresh browser after server restart.

### Kiwix view not loading
- Confirm `public/kiwix-static/www/index.html` exists.
- Check `/api/kiwix/status`.

### Assistant model import fails
- Confirm the model file extension is `.gguf`.
- Ensure the source file starts with GGUF magic bytes.
- Ensure you import from supported source folders (`inbox` or `misc`).

### Assistant load fails or runs slowly
- Check model health in the assistant status line.
- If memory is limited, use a smaller quantized model.
- If tokenizer metadata is missing, re-export the model with tokenizer fields included.
