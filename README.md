# Fyr

Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local web server and works without internet once content is present.

![Fyr Hero](./public/assets/fyr-hero.png)

## Features

- Offline maps with PMTiles
- Library with EPUB, Markdown, and ZIM reading
- Embedded Kiwix web reader for large ZIM archives over HTTP range requests
- Local AI assistant for GGUF models
- Download queue and local content management for maps, books, models, POI, and misc files

## One-Minute Start

```bash
cargo build --release -p server --bin fyr
./target/release/fyr
```

Open http://127.0.0.1:8080.

## Install and Run

For full installation paths (development from source, Docker on existing systems, and Raspberry Pi OS from scratch with Docker), see [INSTALL.md](INSTALL.md).

### Docker (Production)

```bash
docker run --rm -p 8080:8080 \
	-e FYR_HOST=0.0.0.0 \
	-e DATA_DIR=/data \
	-v fyr-data:/data \
	hexagon/fyr:latest
```

Open http://127.0.0.1:8080.

### Docker (Dev Release)

```bash
docker run --rm -p 8080:8080 \
	-e FYR_HOST=0.0.0.0 \
	-e DATA_DIR=/data \
	-v fyr-data:/data \
	hexagon/fyr:dev
```

Note: `hexagon/fyr:dev` is a pre-release tag for testing and validation. Use `hexagon/fyr:latest` for production workloads.

### Manual Build and Run

```bash
cargo build --release -p server --bin fyr
./target/release/fyr
```

On Windows PowerShell:

```powershell
cargo build --release -p server --bin fyr
.\target\release\fyr.exe
```

## Docker Data Behavior

- Mount `DATA_DIR` (default `/data`) as a persistent volume to keep user content across upgrades.
- On startup, Fyr automatically refreshes the two system-managed manuals in `DATA_DIR/books/`:
	- `user-manual.md`
	- `developer-manual.md`
- Other files under `DATA_DIR` are preserved as user-managed content.

## Validation Checks

```bash
cargo test --workspace --all-targets
cargo check -p server
cd crates/ui/frontend && npm ci && npm run build
cd ../../docs/build && npm ci && npm run build
cd ../../docs/build && npm run verify:kiwix
```

## Documentation

- Installation guide: [INSTALL.md](INSTALL.md)
- User guide: [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- Developer guide: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)
- Project governance and module map: [AGENTS.md](AGENTS.md)

## License

This repository is licensed under MIT. See [LICENSE](LICENSE).

## Third-Party Licensing

- Fyr project code is licensed under MIT.
- This repository also distributes an embedded Kiwix web bundle under `public/kiwix-static/`.
- The Kiwix bundle keeps its upstream copyleft licenses and notices:
	- GPLv3 text: `public/kiwix-static/LICENSE-GPLv3.txt`
	- AGPLv3 text (for included replay worker artifact): `public/kiwix-static/LICENSE-AGPLv3.txt`
	- Bundle notice/provenance summary: `public/kiwix-static/THIRD_PARTY_NOTICES.txt`
