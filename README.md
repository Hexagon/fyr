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

## Validation Checks

```bash
cargo test --workspace --all-targets
cargo check -p server
cd crates/ui/frontend && npm ci && npm run build
cd ../../docs/build && npm ci && npm run build
```

## Documentation

- User guide: [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- Developer guide: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)
- Project governance and module map: [AGENTS.md](AGENTS.md)

## License

This repository is licensed under MIT. See [LICENSE](LICENSE).
