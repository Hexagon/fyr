# Fyr User Manual

## 1. What Fyr Is
Fyr is an offline-first content platform for maps, books, and knowledge archives.
It requires no external database, cache server, or other services to run.
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
| Linux / macOS | `curl -fsSL https://fyr.guide/install.sh \| sh` |
| Windows PowerShell | `irm https://fyr.guide/install.ps1 \| iex` |

The scripts persist settings (port, data volume, admin password) in `~/.config/fyr/install.conf` (Linux/macOS) or `%APPDATA%\fyr\install.conf` (Windows). To upgrade to a newer image, add `update`:

```bash
curl -fsSL https://fyr.guide/install.sh | sh -s -- update
```

```powershell
irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Update
```

All arguments are documented inline via `--help` (Linux/macOS) or `-Help` (Windows), and the full reference table is available at [fyr.guide/#installation](https://fyr.guide/#installation).

To serve HTTPS (needed for GPS on phones, see §2a), add `--https` (`-Https` on Windows) for a self-signed certificate, or `--tls-cert <file> --tls-key <file>` (`-TlsCert`/`-TlsKey`) for your own. The choice is remembered for `update`; `--no-https` (`-NoHttps`) turns it off again.

## 2. Start Fyr
For complete installation instructions—including building from source, running via Docker, or setting up a Raspberry Pi—use [fyr.guide/#installation](https://fyr.guide/#installation). (If viewing this file locally, open `docs/site/index.html` in a browser for the same installation guide.)

Once Fyr is running, open `http://localhost:8080` on the same machine, or `http://<host-or-device-ip>:8080` if Fyr runs in Docker or on another device. With HTTPS enabled (§2a) use `https://` instead.

## 2a. HTTPS and Trusting This Server
Browsers only allow the location features (**My Location**, **Record Track**, **Add location** on the Board) on `https://` pages or on `localhost`. Fyr is usually opened over plain `http://` on a LAN or a Raspberry Pi hotspot, where these are blocked. HTTPS also stops a rogue hotspot from altering the Fyr page in transit, which is what keeps encrypted message channels safe against active attackers (§3d), and the admin login cookie is then only ever sent over the encrypted connection.

Fyr can serve HTTPS itself; no separate proxy is needed.

| Setting | Effect |
|---------|--------|
| `FYR_TLS=self-signed` | Fyr creates its own certificate on first start and keeps it in `DATA_DIR/tls/`. Every device shows a one-time warning that you accept (below). |
| `FYR_TLS_CERT` + `FYR_TLS_KEY` | Use your own certificate (PEM chain and private key), for example from your own CA or Let's Encrypt. No warnings if devices trust the issuer. Restart Fyr after renewing it. |
| `FYR_TLS_SANS` | Extra names/IPs for the self-signed certificate, comma-separated (for example `fyr.lan,192.168.4.1`). |
| `FYR_HTTP_REDIRECT_PORT` | Optional: also listen on this port with plain HTTP and send visitors to HTTPS (for example `80`). Off by default. |

Examples:

```bash
# Docker: HTTPS on port 8080, certificate valid for the Pi's hotspot address
docker run -d -p 8080:8080 -e FYR_TLS=self-signed -e FYR_TLS_SANS=192.168.4.1 \
  -v fyr-data:/data --name fyr hexagon/fyr:latest

# Installer (adds the host's name and LAN addresses automatically)
curl -fsSL https://fyr.guide/install.sh | sh -s -- --https
```

Once enabled, the port speaks **only** HTTPS: open `https://<host-or-ip>:8080`. A plain `http://` address on that port no longer loads (use `FYR_HTTP_REDIRECT_PORT` if people have the old address bookmarked).

**Which names the self-signed certificate covers:** `localhost`, `127.0.0.1`, `::1`, the computer's host name and `<hostname>.local`, its LAN IPv4 addresses (and private `fd…` IPv6 addresses) found at startup, plus `FYR_TLS_SANS`. In Docker the container cannot see the host's addresses, so pass them in `FYR_TLS_SANS` (the installers do this for you). The certificate is valid for 825 days. Fyr creates a new one when a new name or address appears, or 30 days before it expires; devices then show the warning once more. The private key (`key.pem`) is readable only by the Fyr user.

### Trust this server (self-signed certificate)
The first time a device opens Fyr over HTTPS it warns that the connection is "not private": the certificate was made by your Fyr server, not by a public authority. Before accepting, **compare its SHA-256 fingerprint** with the real one. Admins find it on the **Overview** page (next to the data directory) and in the server log at startup (`docker logs fyr 2>&1 | grep fingerprint`). If the fingerprints differ, do not continue: someone may be imitating your server (unless the log shows Fyr just created a new certificate).

- **Chrome / Edge (Windows, macOS, Linux, Android):** on the warning page choose **Advanced** → **Proceed to … (unsafe)**. To see the fingerprint, click **Not secure** in the address bar → certificate details (*SHA-256 Fingerprint*).
- **Firefox (desktop and Android):** **Advanced…** → **View Certificate** (check *SHA-256*) → back → **Accept the Risk and Continue**. Firefox remembers the exception.
- **Safari (macOS):** **Show Details** → **View the certificate** to check the fingerprint → **visit this website** → confirm (macOS may ask for your password to remember it).
- **iPhone / iPad (Safari):** **Show Details** → **visit this website** → **Visit Website**. Tapping **View Certificate** on the details page shows the fingerprint on recent iOS versions.

After accepting, the page counts as a secure connection, so the browser can ask for location permission and **My Location** / **Record Track** work. Some browsers ask again after a restart or when the certificate changes; some optional features (such as **Install app**) may still require a fully trusted certificate. For warning-free access on your own devices, use `FYR_TLS_CERT`/`FYR_TLS_KEY` with a certificate they trust, or add `DATA_DIR/tls/cert.pem` to the device's trusted certificates.

With your own certificate (`FYR_TLS_CERT`), Fyr also sends `Strict-Transport-Security`, so browsers that have visited once will insist on HTTPS for this address for 180 days. It is not sent for self-signed certificates.

---

## 3. Main Pages
- **Home:** system status, location, sunrise/sunset, and storage overview.
- **Content Manager:** add URL downloads, import local files, and inspect content inventory. Requires admin access when `FYR_ADMIN_PASSWORD` is set.
- **Settings:** configure location and other application-wide preferences. Requires admin access when `FYR_ADMIN_PASSWORD` is set; hidden in read-only mode.
- **Maps:** map selection, coordinates (decimal/DMS), go-to-coordinate, shareable links, distance measuring, GPS track recording, POI editing and GPX import/export (see *Navigating with maps off-grid*).
- **Books:** browse books, read EPUB/PDF/Markdown/ZIM in a unified reader shell, and use auto-collapse Library focus mode when opening a title.
- **Assistant:** browse local `.gguf` models and chat offline.
- **Search:** one search box for the whole offline library (see [3c. Searching Everything](#3c-searching-everything)).
- **Messages:** a public Board for everyone on the network plus end-to-end encrypted channels protected by a passphrase (see [3d. Messages](#3d-messages)).
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
- Posting in Messages and creating encrypted channels — unless the operator also sets `FYR_GUEST_MESSAGING=true` (see §3d)
- Message moderation (pin/unpin, delete messages and channels)
- Content Manager (downloads, imports, file deletion)
- Settings (location and preferences)
- Model uploads and model management
- Storage details on the Overview page (the server also hides data-directory paths and disk capacity from guests; per-category usage stays visible)

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
Setting `FYR_READONLY=true` disables all mutating operations permanently, including posting in Messages (reading still works). No login is possible. The Content Manager, Settings, and model management links are hidden. The server shows a **Read-only** badge in the navbar.

Use this for kiosk or public library deployments where content is pre-loaded and no runtime management is needed. In this mode the server never reveals filesystem paths or disk capacity to any visitor.

### Install as an app (phones, tablets, desktops)
Fyr ships a web app manifest, so browsers can add it to the home screen or app launcher: use **Add to Home Screen** (iOS Safari, Android Chrome) or **Install app** (desktop Chrome/Edge). It opens full-screen with the Fyr icon and still talks to your Fyr server; it does not work while the server is unreachable.

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
> * Inference runs entirely on CPU with SIMD acceleration: **NEON** on ARM (aarch64, including Raspberry Pi 4/5) and **AVX2/FMA** on x86\_64. No GPU is needed.
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

### Ask your library (answers with sources)

Turn on **Use library** next to the mode buttons to have the assistant answer from your own offline books instead of only from what the model remembers.

- Fyr picks keywords from your question (English and Swedish filler words are ignored) and searches the books folder: **ZIM** archives by article text when the archive has a full-text index (see [Full-text index for ZIM archives](#full-text-index-for-zim-archives)), otherwise by article title (for example *Gustav Vasa* or *Water purification*), **Markdown** files by section, and **EPUB** books by chapter. PDFs are not searched yet.
- The best-matching passages (up to four) are given to the model as numbered sources. The model is asked to cite them as `[1]`, `[2]`, … and to say when your library does not cover the question.
- Sources appear as chips under the answer; clicking a chip or a `[n]` citation in the text opens that article or book in **Books** (ZIM articles open directly; other books open with the matching words searched).
- If nothing matches, the answer shows *No matching passages found in your library.*
- Tips: name the topic explicitly ("Who was Gustav Vasa?" works better than "who was he?"), and prefer article-style titles for ZIM archives. The search is capped at a few seconds, so very large libraries return the best matches found within that time.
- Sources take room in the model's context window. On small windows (2048 tokens) Fyr shortens sources and drops older conversation turns first; your question is always kept.

The **Use library** setting and your chosen mode are remembered per browser.

### Persisting the default model

The last model you selected is remembered in browser storage. When you re-open the Assistant, Fyr will automatically re-select and attempt to load that model. If the load fails (e.g. the model file was removed), an error message is shown in the chat and you can select another model manually.

The conversation (including answer sources) is also kept in browser storage until you press **Restart Conversation**. **Regenerate** replaces the last answer instead of appending a duplicate question. If generation fails, the error is shown in the answer bubble; if the model was unloaded meanwhile, select it again in the model library.

## 3c. Searching Everything
Use the **Search…** box in the header (or press <kbd>/</kbd> anywhere outside a text field) and press Enter to open the **Search** page (`/search?q=...`). One query covers:

| Group | What is matched | Opening a result |
|-------|-----------------|------------------|
| **Books** | Book titles (EPUB/PDF/Markdown/ZIM metadata) and filenames | Opens the book |
| **Articles** | Article titles in every `.zim` archive in `books/` | Opens that article in the ZIM reader |
| **In article text** | Words in ZIM article bodies, for archives with a full-text index (queries of 3+ characters); shows a highlighted excerpt | Opens that article in the ZIM reader with the search term |
| **In book text** | Text inside Markdown and EPUB books (queries of 3+ characters) | Opens the book with the search term |
| **Maps** | Map filenames in `maps/` | Opens that map |
| **Places** | `name` / `title` / `description` of features in GeoJSON files in `poi/` | Opens Maps centred on the place |

- Results are grouped by kind with the most relevant group first; exact and "starts with" matches rank above matches in the middle of a word. Matching ignores upper/lower case and accents (`zurich` finds `Zürich`, `malmo` finds `Malmö`).
- Use the filter chips (All, Books, Articles, Article text, Book text, Maps, Places) to narrow the search.
- Keyboard: <kbd>↓</kbd>/<kbd>↑</kbd> select a result, <kbd>Enter</kbd> opens it, <kbd>Esc</kbd> clears the selection, then the query.
- Search is read-only and available in every access mode, including `FYR_READONLY`.
- Each search has a time limit so very large archives cannot freeze Fyr. Without a full-text index, very large ZIM archives (for example a full Wikipedia) are searched by **title prefix** ("albert ein" finds "Albert Einstein"); smaller archives also match words inside titles. If the limit is reached the page says so; a more specific query helps. The first search after adding a medium-sized archive may return fewer article matches while Fyr indexes it in the background.
- `.fgb` (FlatGeobuf) POI files, Markdown files over 8 MB and EPUBs over 32 MB are not text-searched; GeoJSON files over 32 MB are skipped.

### Full-text index for ZIM archives

Without an index, Fyr can only match ZIM article **titles**. To find words inside articles (and give the assistant better sources), Fyr builds a full-text index per archive in the background:

- Archives with up to 200 000 articles (`FYR_FULLTEXT_MAX_ARTICLES`) are indexed automatically, one at a time, at low priority. Larger archives (for example a full Wikipedia) are only indexed when an admin clicks **Build index**, because that takes hours on a Raspberry Pi and several GB of disk.
- **Content Manager → Books** shows a **Full-text** column for each `.zim` file: *Waiting to index*, *Indexing 42%*, *Paused*, *Full-text ready*, *Index failed* or *Not indexed*. Admins (any visitor in open mode) see **Build index** and **Delete index** buttons; they are hidden for visitors in password-protected and read-only modes, where the server also refuses them.
- Search and the assistant already use a partly built index. Indexing resumes where it stopped after a restart, and starts over automatically when an archive file is replaced. Deleting an index stops automatic indexing of that archive until **Build index** is clicked again; deleting the archive deletes its index.
- Indexing pauses while free disk space is below 1 GB (`FYR_FULLTEXT_MIN_FREE_MB`) and resumes when space is available. The index stores the articles' plain text plus a word index, so it can be larger than the compressed `.zim` file itself (plan for 1–3× the archive size for text-only archives; much less for archives with many pictures).
- Set `FYR_FULLTEXT=off` to disable indexing and full-text search entirely.

## 3d. Messages

Messages has no user accounts. Everyone picks a nickname once per device (stored in the browser, editable at any time). Each device also gets a random **device tag** — a short code shown as a coloured dot next to its messages so you can tell people with the same nickname apart. Nicknames and tags are **not verified**: anyone can type any nickname.

### The Board (public)
The **Board** is always listed first. It is a public chat that everyone who can reach the server can read; the server stores it in plain text.

- Posts can carry a **category** chip — *Info*, *Need*, *Offer* or *Alert* — and an optional **location** (📍). A location links to the Maps page centred on that spot. **Add location** uses the browser's location when the page is served over HTTPS (or localhost); on a plain-HTTP hotspot it falls back to the location saved in Settings.
- Board posts expire after 30 days by default; pick 1 day to 1 year in the composer.
- Admins can **pin** important posts (pinned posts stay at the top and never expire) and **delete** posts.
- Active **Alert** posts also appear as a banner on the Overview page. Each visitor can dismiss them.
- The **Messages** nav link shows how many new Board posts arrived since you last looked.

### Encrypted channels
Open **Join or create an encrypted channel**, enter a channel name and a passphrase, and press **Join**. Your device derives the keys (this takes about a second). If nobody has created that channel yet you are offered **Create this channel**. Everyone who enters the same name and passphrase ends up in the same channel; a different passphrase gives a different, unrelated channel. There is no list of channels on the server.

- Use a strong passphrase — four or more random words. Anyone who captures the channel id on the network can try to guess a weak passphrase offline.
- **Remember on this device** stores the channel keys (never the passphrase) in the browser so the channel stays in your list; **Forget** removes them. Without it the channel lasts until you close the tab.
- **Move channel keys to another device** (below the join form) exports all remembered channel keys as a file encrypted with a passphrase you choose (at least 8 characters), and imports such a file on another device. Without that file passphrase the file is useless; a wrong passphrase or a modified file is rejected. The keys only work on the same Fyr server (or one restored from its backup, §6a), because channel ids depend on the server.
- **Share** gives a link (`/messages#join=<name>`) and a QR code. The part after `#` never reaches the server. You can choose to include the passphrase in the link — anyone who sees that link or QR code can then read and post, so share it only in person. Opening such a link only fills in the form; the passphrase is removed from the address bar immediately.
- Messages are end-to-end encrypted on your device; the server only stores scrambled data. Channels keep the newest 1000 messages and messages younger than 90 days. Channels with no messages are removed after a week of inactivity — anyone with the name and passphrase can simply create them again (**Create it again**).
- Admins can delete a channel and its messages, but cannot read them.

### What the encryption protects (threat model)
- **Protects against:** people passively sniffing the Wi-Fi, and anyone who copies or reads the server's disk.
- **Does not protect against:** an active attacker who can change the web page code while it travels over plain HTTP (for example a rogue hotspot imitating yours). Serve Fyr over **HTTPS** (built in, see §2a, or through a reverse proxy) if that matters to you.
- Anyone who knows the passphrase can read everything in the channel and post under any nickname.
- The Board is never encrypted.

### Who can post
| Mode | Read Board / channels | Post and create channels | Pin / delete |
|------|-----------------------|--------------------------|--------------|
| Open (default) | Everyone | Everyone | Everyone (there is no admin login) |
| `FYR_ADMIN_PASSWORD` | Everyone | Admin only | Admin only |
| `FYR_ADMIN_PASSWORD` + `FYR_GUEST_MESSAGING=true` | Everyone | Everyone | Admin only |
| `FYR_READONLY` | Everyone | Nobody | Nobody |

Posting is rate-limited per device address (a short burst, then about 15 messages per minute; channel creation about one per minute). Behind a reverse proxy all visitors share the proxy's address, so these limits are shared too — unless you list the proxy in `FYR_TRUSTED_PROXIES` (§6).

Set `FYR_MESSAGING=false` to switch Messages off entirely.

## 4. Add Content
### Data directories and supported file types
All data is stored under `public/data/` (or `DATA_DIR` if you override it).

| Folder | Supported file types | Typical use |
| --- | --- | --- |
| `curated-content.json` | structured JSON catalog | Manually editable list of recommended model, book, and map downloads |
| `books/` | `.epub`, `.pdf`, `.mobi`, `.md`, `.zim` | Offline books, manuals, and archives (`.mobi` files are accepted for storage but have no in-browser reader) |
| `maps/` | `.pmtiles`, `.mbtiles` | Offline map tiles |
| `poi/` | `.geojson`, `.fgb`, `.json` | POI layers and geo datasets |
| `models/` | `.gguf` | Local AI models for Assistant |
| `misc/` | `.txt`, `.csv`, `.zip`, `.7z`, `.log`, `.exe`, `.msi`, `.deb`, `.rpm`, `.dmg`, `.pkg` | General offline resources and installers |
| `inbox/` | temporary files during upload/import | Staging area used by import workflows before auto-routing |

### Books
- Put local book files in `public/data/books/`.
- URL downloads with supported book extensions are routed to `books/` automatically.
- For ZIM archives, use trusted OpenZIM-compatible sources.

### Maps
- Fyr serves vector and raster map tiles from `.pmtiles` and `.mbtiles` archives in `public/data/maps/`.
- PMTiles archives are read directly by the browser without a tile server.
- MBTiles archives (SQLite-based) are served tile-by-tile through the built-in tile endpoint (`/api/maps/tiles/{filename}/{z}/{x}/{y}`).

**Obtaining PMTiles maps:**

**Pre-compiled Shortbread maps (BBBike):**
Download ready-to-use `.pmtiles` files from [data.bbbike.org](https://data.bbbike.org/osm/region/).

**Extract from Protomaps (CLI):**
Install the `pmtiles` CLI ([docs.protomaps.com/pmtiles/cli](https://docs.protomaps.com/pmtiles/cli)) and extract a region from a global PMTiles archive. Find the latest dated build at [maps.protomaps.com/builds](https://maps.protomaps.com/builds/) and substitute it into the URL below:

```
pmtiles extract https://build.protomaps.com/<YYYYMMDD>.pmtiles sweden.pmtiles --bbox=4.7,55.0,24.2,69.1 --maxzoom=15
pmtiles extract https://build.protomaps.com/<YYYYMMDD>.pmtiles world.pmtiles --bbox=-180,-85.0511,180,85.0511 --maxzoom=8
```

### Navigating with maps off-grid
The **Maps** page is built to be usable for navigation without network access.

- **Opening a map:** pick a map in *Available Maps*. When nothing else positions the view, the map opens on its own coverage area (or on your saved location if that lies inside it). The selected map is written to the address bar (`/maps?map=<file>`), so the view can be bookmarked or shared.
- **Coordinates:** the bottom status bar shows the coordinate under the mouse pointer (🖱) or, on touch screens, the map centre (⊕). Click it to copy it. The **DEC/DMS** button switches between decimal degrees (`59.32930, 18.06860`) and degrees-minutes-seconds (`59°19'45.5"N 18°04'07.0"E`); the choice is remembered in the browser. A scale bar is shown in the lower-right corner.
- **Click any point** on the map (with no tool active) to see its coordinate in both formats with **Copy** and **Copy link** buttons. Clicking a POI or track shows its name (and length for tracks).
- **Go to coordinate:** in *Tools*, type a coordinate and press **Go**. Accepted forms include `59.33, 18.07`, `59.33 18.07`, `-33.86, 151.21`, `59,33 18,07`, `59.33N 18.07E`, `59°19'48"N 18°4'12"E`, `N 59° 19.8' E 18° 4.2'` and `59 19 48, 18 4 12`. A red marker is placed there; if the current map does not cover the point Fyr switches to an installed map that does. Remove the marker with ✕.
- **Links to a place:** other pages (and people) can link to `/maps?lat=59.33&lng=18.07&zoom=14&label=Camp`. All parameters are optional; `map=<file>` selects a specific map. Without `map`, Fyr picks the most detailed installed map whose coverage contains the point.
- **My Location / Record Track** need the browser's location permission **and a secure connection**: open Fyr through `https://…` or `http://localhost`. Browsers block GPS on plain `http://` LAN addresses (for example `http://192.168.1.10:8080`); Fyr then shows an explanation instead of failing silently. To fix this on a LAN, enable HTTPS (§2a).
- **Record Track:** press **⏺ Record Track** to log your GPS position (inaccurate fixes and jitter under a few metres are ignored). Distance, duration and point count update live and the track is drawn in red. Recording keeps running while you visit other Fyr pages, but keep the browser tab open and the screen on (Fyr requests a screen wake lock where supported). **⏹ Stop Recording** keeps the track: name it and **Save** it into a POI file (admins; default `tracks.geojson`, timestamps are kept), download it as **GPX**, or **Discard** it. An unsaved track survives a page reload and is offered again.
- **GPX import/export:** open **Edit POIs** (**View POIs** for guests). **⬆ Import GPX** reads a `.gpx` file in the browser: waypoints become points, tracks and routes become lines (each track segment is its own line; elevation and timestamps are kept). With a POI file selected the data is appended to it; otherwise a new, not yet saved file named after the GPX is created. **⬇ GPX** and **⬇ GeoJSON** download the selected file. Polygons cannot be expressed in GPX and are skipped (the panel says how many).
- **Read-only/guest users** can view POI files, tracks and imported GPX data, measure distances, record and download tracks, and export GPX/GeoJSON. Creating files, placing or removing POIs and saving require admin access (the server rejects saves in read-only mode regardless).

### POI
- Put `.geojson`, `.fgb`, or `.json` POI datasets in `public/data/poi/`.
- If using `.json`, make sure it follows a valid geo dataset structure used by your workflow.
- GeoJSON files may contain points, lines (tracks/routes) and polygons; all three are drawn on the map when the file is selected in the POI panel. Saved tracks carry `coordTimes` (one ISO timestamp per point), `time`, `endTime` and `distanceKm` properties.

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
- Use the **Download** button in each Content Manager file row to download a local copy from the browser. Files are streamed from disk with the original filename, so even multi-gigabyte ZIM archives and models download without loading into server memory, and browsers or download managers can resume an interrupted transfer.
- Downloads are auto-routed by recognized extension to the correct folder.
- If a URL points to an unrecognized extension, the file remains in `inbox/` until you move it manually.
- The task list persists across restarts.
- **Resumable URL downloads** (for flaky off-grid links): when a URL download drops mid-transfer, Fyr keeps the bytes it already has and automatically continues from that point (HTTP `Range`) instead of starting over. If the link stays down, or Fyr is stopped or restarted mid-download, the task is marked **failed** but keeps its partial file; the task shows how much was kept (e.g. "1.2 GB of 4 GB kept") and a **Resume** button. Click it to continue where it stopped. Servers that do not support resuming, or whose file changed in the meantime, are downloaded again from the start automatically.
- Stopping Fyr cleanly (Ctrl+C, `docker stop`, or a service stop) saves the progress of running downloads so they can be resumed after the next start.
- Failed or cancelled URL downloads without kept data show a **Retry** button that starts them again. Dismissing a failed task deletes any kept partial data; cancelling a running download deletes it as well. Local imports that were interrupted must be re-imported.
- **Checksum verification**: enter an optional **SHA-256 checksum** next to the URL field. After the download finishes, Fyr verifies the file (status **verifying**) before moving it into place; on a mismatch the task fails with "checksum mismatch" and the file is removed. Curated catalog entries that include a `sha256` value are verified automatically when you click **Download now**.
- Resume, Retry and other download actions require admin access when Fyr runs with `FYR_ADMIN_PASSWORD`, and are unavailable in `FYR_READONLY` mode.
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

## 5d. Reading Progress, Bookmarks and Links
- **Resume where you left off:** Fyr remembers your last position per book: the EPUB location, the ZIM article, and the Markdown section. Opening the book again takes you back there. PDF files open inline through the browser's PDF viewer, so Fyr cannot see the page number and starts from the first page.
- **Bookmarks:** use **＋ Bookmark** in the reader toolbar (or press `b`) to save the current position. **Bookmarks (n)** lists saved bookmarks. Click one to jump to it, or use ✕ to remove it. Bookmarks work for EPUB, Markdown and ZIM.
- **Continue reading:** the Overview page lists your recently opened books that are still in the library. Click a card to resume reading.
- **Search inside a book:** EPUB, Markdown and ZIM have a search box above the reader. Clicking an EPUB result opens its chapter, and clicking a Markdown result scrolls to the matching section and highlights it.
- **ZIM history:** the ‹ / › buttons (or `[` / `]`) move back and forward between articles you opened in the current archive.
- **Text size and theme:** for EPUB and Markdown, use A− / A+ (or `-` / `+`) to change the text size, and pick a Light, Sepia or Dark theme. These settings are saved in your browser.
- **Links to books:** the address bar follows what you are reading, so you can bookmark or share the link, and reloading the page reopens the same book and article:
  - `/books?file=<filename>` opens a book.
  - `/books?file=<archive>.zim&path=<article path>` opens a specific ZIM article.
  - `/books?file=<filename>&q=<words>` opens a book and runs an in-book search.
  - The **←** button (Back to library) closes the book and removes these parameters from the address.
- Keyboard shortcuts do nothing while you are typing in a text field. They also do not work while focus is inside the book or article content. Click the toolbar area first to use them.
- **Privacy and storage:** progress, bookmarks and display settings are stored only in this browser's local storage. They are not stored on the server, so they also work for read-only guests. They do not sync between browsers or devices, and clearing site data removes them. Fyr keeps the 200 most recently used books.
- **Move to another device:** in the Library panel open **Move reading progress**. **Export** saves a JSON file with your positions and bookmarks; **Import** on the other device merges it: books you do not have yet are added, the newer position of each book wins, and bookmarks are combined without duplicates. Nothing is deleted by an import.

## 6. Data Storage Layout
`public/data/` is created automatically and contains the following directories:

- `public/data/maps/`
- `public/data/books/`
- `public/data/poi/`
- `public/data/inbox/`
- `public/data/models/`
- `public/data/misc/`

Messages are stored in `messages.sqlite` directly in the data directory (owner-readable only, never published).

Two more folders may appear briefly: `.backup-tmp/` (temporary files while a backup or restore runs; removed automatically) and `restore-pending/` (files from a restore that are applied at the next start, see §6a).

Full-text indexes of ZIM archives are stored in `index/` in the data directory (one `.fts.sqlite` file per archive, never published). They can be deleted at any time; Fyr rebuilds them as described in [Full-text index for ZIM archives](#full-text-index-for-zim-archives).

Only `maps/`, `books/`, `poi/` and `curated-content.json` are published by the web server under `/data/...` (used for map, reader and POI loading). `settings.json`, `download_tasks.json`, `inbox/`, `models/` and `misc/` are never served directly; models and misc files are downloaded through Content Manager's **Download** button instead.


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
- `FYR_GUEST_MESSAGING` — with `FYR_ADMIN_PASSWORD` set, let guests post in Messages and create encrypted channels (see §3d)
- `FYR_MESSAGING` — set to `false` to disable Messages (default on)
- `FYR_MESSAGES_MAX_PER_CHANNEL` — newest messages kept per channel (default 1000)
- `FYR_MESSAGES_MAX_AGE_DAYS` — maximum age of encrypted-channel messages in days (default 90)
- `FYR_TRUSTED_PROXIES` — comma-separated IPs or CIDR blocks of your reverse proxy (for example `127.0.0.1` or `172.16.0.0/12` for a Docker network). Only for connections from these addresses does Fyr read the visitor's address from `X-Forwarded-For`, so login and messaging rate limits apply per visitor instead of to everyone behind the proxy. Leave unset when Fyr is reached directly.
- `FYR_TLS` — `self-signed` to serve HTTPS with a generated certificate (see §2a); `off` (default) for plain HTTP
- `FYR_TLS_CERT`, `FYR_TLS_KEY` — paths to your own PEM certificate chain and private key; set both to serve HTTPS with them
- `FYR_TLS_SANS` — extra comma-separated host names/IPs for the self-signed certificate
- `FYR_HTTP_REDIRECT_PORT` — optional plain-HTTP port that redirects to HTTPS (requires HTTPS; must differ from `FYR_PORT`)
- `FYR_FULLTEXT` — set to `off` to disable the ZIM full-text index and full-text search (default on)
- `FYR_FULLTEXT_MAX_ARTICLES` — archives with at most this many articles are indexed automatically (default 200000; `0` = only when an admin clicks **Build index**)
- `FYR_FULLTEXT_MIN_FREE_MB` — indexing pauses while free disk space is below this many MB (default 1024)
- `FYR_AI_THREADS` — optional override for the AI inference thread pool size (default: number of available CPU cores, respecting container CPU quotas)

`FYR_HOST` changes where the server listens. Keep `127.0.0.1` for local-only access, or use `0.0.0.0` when Fyr runs in Docker or should accept LAN traffic. In the browser, use the host machine's name or IP address together with `FYR_PORT`.

## 6a. Backup and Restore

Admins can back up and restore the server's own data from **Settings → Backup & restore** (the section is only shown to admins; in read-only mode it is not available at all).

**What is in a backup:** settings, POI files (`.geojson`/`.json`, and `.fgb` files up to 16 MB), the curated catalog, the download history and all Messages (the Board in plain text, encrypted channels still encrypted, plus the server salt that channel ids depend on). Maps, books, models, misc files and the inbox are **not** included: copy the data folder (or Docker volume) for those. The backup is a `.zip` with a `manifest.json` listing every file with its SHA-256 checksum.

Keep backup files private: they contain every Board post with nicknames and locations, and anyone holding one can try to guess weak channel passphrases offline.

**Download backup** creates the file. Taking a backup is safe while people are posting; Fyr copies the messages database consistently.

**Restore:** choose a backup `.zip`, press **Restore backup** and confirm. Fyr checks the whole file first (checksums, file names, sizes, content) and refuses damaged, modified or foreign files without changing anything. Then:

- settings, the curated catalog and POI files from the backup are replaced **immediately** (other POI files that are not in the backup are kept);
- Messages and the download history are replaced at the **next restart** of Fyr. Until you restart, the old messages stay in use. Restart with `docker restart fyr`, `systemctl restart fyr` or by stopping and starting the program.

Uploads are limited to 512 MB. Restoring a backup from a newer Fyr version into an older one is refused if the messages database format is newer.

Via the API (for scripts): `GET /api/backup` and `POST /api/restore` (multipart field `file`) with an admin session cookie. Without `FYR_ADMIN_PASSWORD`, anyone who can reach the server can use them, like every other admin function.

## 6b. Upgrading from 0.5.x to 1.0

1. **Take a backup first.** On 0.5.x there is no backup button: stop Fyr and copy the data folder (`public/data/` or your `DATA_DIR`), or for Docker copy the volume, for example:
   ```sh
   docker stop fyr
   docker run --rm -v fyr-data:/data -v "$PWD":/backup alpine tar czf /backup/fyr-data-0.5.tar.gz -C /data .
   ```
   After the upgrade, use **Settings → Backup & restore** for future backups.
2. **Replace the program or image.** Docker: `docker pull hexagon/fyr:latest`, then remove and re-create the container with the **same** volume (`-v fyr-data:/data -e DATA_DIR=/data`). The image is multi-arch: the same tag works on Intel/AMD (`x86_64`) and ARM64 (`aarch64`, for example Raspberry Pi 4/5 with a 64-bit OS); Docker picks the right one. A Docker volume holds plain files and is not tied to a CPU architecture, so you can also move a volume (or a backup) between x86_64 and aarch64 machines. Binary installs: download the build for your architecture and start it with the same `DATA_DIR`.
3. **Start it.** Existing maps, books, POIs, models and settings are used as they are; no conversion is needed.

**New in the data folder:** `messages.sqlite` (Messages, created on first start, owner-readable only) and, only with `FYR_TLS=self-signed`, `tls/` (the generated certificate and its private key; not included in Settings backups, a new certificate is created if it is missing). Other 1.0 features may add their own folders (for example a search `index/`); all of them live inside the data folder, so keep mounting the whole folder.

**New environment variables** (all optional; defaults keep 0.5.x behaviour except that Messages is on):

- `FYR_MESSAGING` — `false` turns Messages off.
- `FYR_GUEST_MESSAGING` — with `FYR_ADMIN_PASSWORD`, let guests post and create encrypted channels.
- `FYR_MESSAGES_MAX_PER_CHANNEL` — newest messages kept per channel (default 1000).
- `FYR_MESSAGES_MAX_AGE_DAYS` — maximum age of encrypted-channel messages (default 90).
- `FYR_TRUSTED_PROXIES` — set this if Fyr runs behind a reverse proxy, so rate limits apply per visitor.
- `FYR_TLS`, `FYR_TLS_CERT`/`FYR_TLS_KEY`, `FYR_TLS_SANS`, `FYR_HTTP_REDIRECT_PORT` — built-in HTTPS (§2a), off by default. Turning it on changes the address to `https://`.

If you expose Fyr to people you do not fully trust, review `FYR_ADMIN_PASSWORD`/`FYR_READONLY` (§3a) and the Messages posting rules (§3d) after upgrading.

**Going back to 0.5.x:** stop Fyr and restore the copy of the data folder from step 1. 0.5.x ignores `messages.sqlite`.

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
- `Invalid HTTPS configuration` or `Cannot read TLS certificate/private key`: check the `FYR_TLS*` settings (§2a). Fyr refuses to start rather than silently falling back to plain HTTP. In Docker, the container user (UID 1000) must be able to read mounted certificate and key files.

### Page does not load after enabling HTTPS
- Use `https://` in the address; with HTTPS on, the port no longer answers plain `http://`.
- A certificate warning on each device is expected with `FYR_TLS=self-signed` (§2a). If the warning says the name does not match, add the address you use to `FYR_TLS_SANS` and restart.

### Content not visible in UI
- Verify file extension is supported.
- Verify file is in the correct `data/` subfolder.
- Refresh browser after server restart.
- Missing content after a restart generally means you need to confirm the `/data` volume is mounted.

### Download is stuck or failed
- Open Content Manager and inspect the download status/error line.
- Cancel the task and retry the URL.
- A task that shows "interrupted by server restart – retry to resume" (or "…server shutdown…") was running when Fyr stopped; click **Resume** to continue from the kept bytes.
- A task that failed after several attempts on an unreliable link keeps its data too; click **Resume** once the connection is back.
- "checksum mismatch" means the downloaded file differs from the expected SHA-256: check the checksum value and the source, then retry.
- For repeated failures, verify the source URL is reachable and supports direct file transfer.

### ZIM view not loading
- Confirm the selected `.zim` file exists under `public/data/books/` (or your configured `DATA_DIR/books/`).
- Check the server status and retry opening the archive.

### Assistant model import fails
- Confirm the model file extension is `.gguf`.
- Model filenames must be plain file names (no folders or `..`); otherwise the server rejects the request with "invalid model filename".
- Ensure the source file starts with GGUF magic bytes.
- Retry the upload if the browser was interrupted before the file finished transferring.

### Assistant inference fails after load
- Confirm the model architecture is currently supported by Fyr inference.
- Confirm the `.gguf` model includes tokenizer metadata.
- For `phi3`/`phi-3.5`, confirm a sidecar tokenizer JSON exists next to the `.gguf` model.
- If the model still loads but will not generate text, check the assistant status line for tokenizer or runtime errors.

### "Use library" finds no sources
- Make sure the books are in the `books/` data folder (ZIM, EPUB or Markdown; PDFs are not searched).
- For ZIM archives without a full-text index, ask about the topic by its article title; check the **Full-text** column in Content Manager → Books and build the index for large archives to search article text.
- Very large libraries are searched for a few seconds at most; ask a more specific question to narrow it down.

### Assistant load fails or runs slowly
- Check model health in the assistant status line.
- If memory is limited, use a smaller quantized model.
- If tokenizer metadata is missing, re-export the model with tokenizer fields included.
- For `phi3`/`phi-3.5`, use the original `tokenizer.json` from the model repository and place it beside the model file.
- On constrained hardware (e.g. Raspberry Pi), check the server startup logs for CPU feature information. If the log shows `dotprod` is available at runtime but not compiled in, rebuild the Docker image with `--build-arg RUST_TARGET_FEATURES=+dotprod` for faster quantized inference. You can also adjust the inference thread pool by setting `FYR_AI_THREADS=<n>` (default: number of available CPU cores).

## 9. Tools

The **Tools** page (accessible from the top navigation bar) provides common offline utilities organized into two tabs. All operations stay local to your Fyr deployment (offline-safe): some run in-browser, and some are processed by Fyr's local API.

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

> **Implementation notes:**
> * Unit converters, Base64, and ROT13 run directly in the browser.
> * AES and hash/checksum operations are handled by Fyr's local server API.
> * Outputs use standard text encodings and hash formats shown in the UI.
>
> **Security note:** The AES tool is designed for convenience and casual use. For high-security applications, use purpose-built encryption tools with audited key management. MD5 and SHA-1 are cryptographically broken and should not be used for security purposes; they are included for legacy checksum verification.
