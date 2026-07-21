# Fyr

Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local web server and works without internet once content is present.

## Features

- Offline maps with PMTiles
- Library with EPUB, PDF, Markdown, and ZIM reading
- Native Fyr ZIM reader service with server-side archive access
- Local AI assistant for GGUF models
- Download queue and local content management for maps, books, models, POI, and misc files

## One-Minute Start

If you have Rust installed and need a quick local launch, you can build Fyr from source:

```bash
cargo build --release -p server --bin fyr
./target/release/fyr
```

Open `http://localhost:8080` on the same machine.

## Installation & Deployment

For comprehensive installation paths, please refer to [fyr.guide/#installation](https://fyr.guide/#installation). It serves as the single source of truth for deployment and includes step-by-step guides for:

* Building from source for development workflows.
* Running with Docker (both Production and Dev releases) on an existing system.
* Installing and running Fyr on a clean Raspberry Pi OS setup.

## Data Management

Fyr stores all user content in a persistent data directory (defaulting to `./public/data` locally or `/data` in Docker).

To learn how to persist data across reinstalls, refer to the volume mounting instructions at [fyr.guide/#installation](https://fyr.guide/#installation). For a detailed breakdown of folder structures, supported file types, and how Fyr handles automatic system-manual refreshes on startup, see the Data Storage Layout section in the [User Manual](/docs/user/USER_MANUAL.md).

## Documentation

- Installation guide: [fyr.guide/#installation](https://fyr.guide/#installation)
- User guide: [docs/user/USER_MANUAL.md](/docs/user/USER_MANUAL.md)
- Developer guide: [docs/developer/DEVELOPER_MANUAL.md](/docs/developer/DEVELOPER_MANUAL.md)

## License

This repository is licensed under MIT. See [LICENSE](LICENSE).