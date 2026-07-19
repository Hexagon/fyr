# Fyr

Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local web server and works without internet once content is present.

![Fyr Hero](./public/assets/fyr-hero.png)

## Features

- Offline maps with PMTiles
- Library with EPUB, PDF, Markdown, and ZIM reading
- Native Fyr ZIM reader service with server-side archive access
- Local AI assistant for GGUF models
- Download queue and local content management for maps, books, models, POI, and misc files

## One-Minute Start

```bash
cargo build --release -p server --bin fyr
./target/release/fyr
```

Open http://localhost:8080 on the same machine, or `http://<host-or-device-ip>:8080` when Fyr is published from Docker or another host.

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

Open http://localhost:8080 on the same machine, or `http://<host-or-device-ip>:8080` from another device.

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

`FYR_HOST` controls the bind address. Keep the default `127.0.0.1` for local-only access, or set `FYR_HOST=0.0.0.0` when Fyr runs in Docker or should be reachable from other devices. In a browser, use the machine or device address that publishes port `8080`; do not use the container bind address itself.

## Docker Data Behavior

- Mount `DATA_DIR` (default `/data`) as a persistent volume to keep user content across upgrades.
- Use either Docker named volumes (`-v fyr-data:/data`) or host folder bind-mounts (`-v /path/to/fyr-data:/data`).
- On startup, Fyr automatically refreshes the two system-managed manuals in `DATA_DIR/books/`:
	- `user-manual.md`
	- `developer-manual.md`
- Other files under `DATA_DIR` are preserved as user-managed content.
- Reusing the same mount path/volume across reinstalls keeps your data.

## Documentation

- Installation guide: [INSTALL.md](INSTALL.md)
- User guide: [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- Developer guide: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)
- Project governance and module map: [AGENTS.md](AGENTS.md)

## License

This repository is licensed under MIT. See [LICENSE](LICENSE).