# Fyr

Fyr is an offline-first content platform for maps, books, and knowledge archives.
It runs as a local web server and works without internet once content is present.

> **Project status:** Fyr is currently in **preview**. Interfaces and workflows may continue to evolve.

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

Prefer Docker for first-time evaluation? Use the install paths on [fyr.guide/#installation](https://fyr.guide/#installation).

## Installation & Deployment

The canonical installation guide lives at [fyr.guide/#installation](https://fyr.guide/#installation) and includes:

* Building from source for development workflows.
* Running with Docker (both Production and Dev releases) on an existing system.
* Installing and running Fyr on a clean Raspberry Pi OS setup.

## Documentation Map

- Website + installation playbooks: [fyr.guide](https://fyr.guide/)
- End-user operations and troubleshooting: [/docs/user/USER_MANUAL.md](/docs/user/USER_MANUAL.md)
- Architecture and implementation details: [/docs/developer/DEVELOPER_MANUAL.md](/docs/developer/DEVELOPER_MANUAL.md)
- Contributor workflow and validation requirements: [/CONTRIBUTING.md](/CONTRIBUTING.md)
- Repository governance and ownership boundaries: [/AGENTS.md](/AGENTS.md)

## Data Management

Fyr stores user content in a persistent data directory (`./public/data` by default, `/data` in Docker).
See [fyr.guide/#installation](https://fyr.guide/#installation) for persistence setup and the [User Manual data layout section](/docs/user/USER_MANUAL.md#6-data-storage-layout) for folder-level details.

## Documentation

- Installation guide: [fyr.guide/#installation](https://fyr.guide/#installation)
- User guide: [/docs/user/USER_MANUAL.md](/docs/user/USER_MANUAL.md)
- Developer guide: [/docs/developer/DEVELOPER_MANUAL.md](/docs/developer/DEVELOPER_MANUAL.md)
- Contributing: [/CONTRIBUTING.md](/CONTRIBUTING.md)

## License

This repository is licensed under MIT. See [LICENSE](LICENSE).