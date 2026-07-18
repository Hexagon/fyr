# Fyr

Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local web server and works without internet once content is present.

![Fyr Hero](./public/assets/fyr-hero.png)

## Quick Start

### Run Local Binary
```bash
cargo build --release -p server --bin fyr
./target/release/fyr
```

Open http://127.0.0.1:8080.

## Features

### Offline AI Assistant
Fyr supports local GGUF models for offline assistant workflows.
Inference runs locally without internet access when a model is loaded.

Quickstart:
1. Place `.gguf` files in `public/data/models/` (or your `DATA_DIR/models/`).
2. Open the Assistant tab.
3. Load a model and start chatting.

### Run Docker Image
```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

## Data Layout

Fyr now uses a public-first layout:

- `public/static/` - built SPA assets
- `public/kiwix-static/` - embedded Kiwix reader assets
- `public/assets/` - static project assets
- `public/data/` - default content storage

Content subfolders:

- `public/data/maps/`
- `public/data/books/`
- `public/data/poi/`
- `public/data/inbox/`
- `public/data/models/`
- `public/data/misc/`

Environment overrides:

- `DATA_DIR` (default: `./public/data`)
- `FYR_HOST` (default: `127.0.0.1`)
- `FYR_PORT` (default: `8080`)

## Documentation

Canonical docs are intentionally limited to:

- [README.md](README.md)
- [AGENTS.md](AGENTS.md)
- [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)

Other files under [docs/](docs/) are retained only as migration stubs.

## Platform Support Notes

- `x86_64` and `aarch64` are primary runtime targets.
- Raspberry Pi (64-bit OS) is supported via native ARM64 builds or multi-arch Docker images.
- For network deployments, set `FYR_HOST=0.0.0.0`.

## License

This repository is licensed under MIT. See [LICENSE](LICENSE).
