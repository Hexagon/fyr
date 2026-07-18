# Fyr UI

A lightweight Vue 3 SPA frontend for Fyr.

## Development

```bash
cd crates/ui/frontend
npm install
npm run dev
```

Then open http://localhost:5173 (Vite dev server will proxy `/api` to http://localhost:8080)

## Building

```bash
npm run build
```

Outputs to `static/` directory, which is served by the Rust server.

## Project Structure

- `src/main.js` — App entry point
- `src/App.vue` — Main layout + navigation
- `src/pages/` — Home, ContentManager, Maps, Books pages
- `src/services/api.js` — API client
- `vite.config.js` — Build configuration

## Features

- **Home** — Dashboard with server status and content inventory
- **Content Manager** — Download content, view active downloads, manage files
- **Maps** — Browse and view offline maps (PMTiles support)
- **Books** — Search and read books collection (EPUB, PDF, ZIM, text)

## Technologies

- Vue 3 (lightweight, ~100KB gzipped)
- Vite (fast build, ~500ms)
- Axios (HTTP client)
- MapLibre GL + PMTiles (map viewer)
- TailwindCSS (styling, minimal)

## Build Output

- CSS files: `static/css/`
- JS files: `static/js/`
- Total size: ~500KB (gzipped ~150KB)
