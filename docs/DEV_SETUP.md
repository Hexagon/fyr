# DEV_SETUP.md — Developer Setup Guide

## Overview

This guide covers setting up the Offline Nexus development environment for contributing to the project.

## Prerequisites

- **Rust 1.70+**: Install from [rustup.rs](https://rustup.rs/)
- **Git**: Version control
- **Operating System**: Linux, macOS, or Windows (with WSL2 recommended)

## Initial Setup

### 1. Clone Repository

```bash
git clone https://github.com/Hexagon/offline-nexus.git
cd offline-nexus
```

### 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup update
```

Verify installation:
```bash
rustc --version
cargo --version
```

### 3. Verify Build

```bash
cargo build
# Takes ~2-3 minutes on first build (downloading dependencies)
```

Result: `target/debug/nexus`

## Development Workflow

### Running Development Server

```bash
cargo run --bin nexus
# Server starts on http://localhost:8080
```

With debug logging:
```bash
RUST_LOG=debug cargo run --bin nexus
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p downloader

# Run with output
cargo test -- --nocapture

# Run single test
cargo test -p types validation::validate_file
```

### Building Release Binary

```bash
cargo build --release
# Result: target/release/nexus (~1.5 MB)
```

## Project Structure

```
offline-nexus/
├── crates/
│   ├── types/                   # Shared types, config
│   │   ├── src/
│   │   │   ├── lib.rs           # Module exports
│   │   │   ├── types.rs         # Core types (ContentType, DownloadTask)
│   │   │   ├── config.rs        # Configuration management
│   │   │   └── validation.rs    # File validation logic
│   │   └── Cargo.toml
│   │
│   ├── server/                  # Axum API server
│   │   ├── src/
│   │   │   ├── main.rs          # Server entry point, router setup
│   │   │   ├── handlers.rs      # HTTP request handlers
│   │   │   └── state.rs         # Shared app state
│   │   └── Cargo.toml
│   │
│   ├── downloader/              # Content ingestion
│   │   ├── src/
│   │   │   ├── lib.rs           # Module exports
│   │   │   ├── download.rs      # HTTP download engine
│   │   │   ├── router.rs        # File routing logic
│   │   │   └── manager.rs       # Download task queue
│   │   └── Cargo.toml
│   │
│   └── ui/                      # Frontend build coordination
│       ├── src/
│       │   └── lib.rs
│       └── Cargo.toml
│
├── docs/                        # All markdown documentation
│   ├── ARCHITECTURE.md
│   ├── API_REFERENCE.md
│   ├── DATA_FORMATS.md
│   ├── CONTENT_DOWNLOADER.md
│   ├── DEPLOYMENT.md
│   ├── RADIO_PLANNING.md
│   └── DEV_SETUP.md (this file)
│
├── Cargo.toml                   # Workspace manifest
├── Cargo.lock                   # Dependency lock file
├── .gitignore
├── README.md
├── AGENTS.md
└── Dockerfile
```

## Key Modules & Their Responsibilities

### types
**Purpose**: Shared types and configuration

**Key Files**:
- `types.rs`: `ContentType`, `DownloadTask`, `DownloadStatus`, `ContentMetadata`
- `config.rs`: `Config` struct, directory initialization
- `validation.rs`: File validation logic (warn-only strategy)

**When to modify**:
- Adding new content types
- Changing download task structure
- Updating validation rules

### server
**Purpose**: Axum web server and API endpoints

**Key Files**:
- `main.rs`: Server setup, router configuration
- `handlers.rs`: HTTP request handlers for all endpoints
- `state.rs`: `AppState` definition

**When to modify**:
- Adding new API endpoints
- Changing response formats
- Modifying UI serving logic

### downloader
**Purpose**: Content download and file organization

**Key Files**:
- `download.rs`: HTTP download engine
- `router.rs`: File routing by type
- `manager.rs`: Download task queue

**When to modify**:
- Adding new download sources
- Changing file routing logic
- Updating download progress tracking

## Common Development Tasks

### Adding a New API Endpoint

1. **Define types** in `types/src/types.rs` (if needed)
2. **Create handler** in `server/src/handlers.rs`
3. **Add route** to router in `server/src/main.rs`
4. **Test locally**: `cargo run --bin nexus`
5. **Update docs**: [API_REFERENCE.md](./docs/API_REFERENCE.md)

**Example**: Adding `GET /api/content/maps/search`

```rust
// 1. In server/src/handlers.rs
pub async fn search_maps(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Json<Vec<ContentMetadata>> {
    // Implementation
}

// 2. In server/src/main.rs
.route("/api/content/maps/search", get(handlers::search_maps))

// 3. Test
curl "http://localhost:8080/api/content/maps/search?q=europe"
```

### Adding a New Content Type

1. **Add to `ContentType` enum** in `types/src/types.rs`
2. **Add file routing** in `downloader/src/router.rs`
3. **Add validation** in `types/src/validation.rs`
4. **Update tests**: `cargo test -p downloader`
5. **Document** in [DATA_FORMATS.md](./docs/DATA_FORMATS.md)

### Adding Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_routing() {
        let file = Path::new("test.pmtiles");
        let dest = ContentRouter::route_file(file, Path::new("/data")).unwrap();
        assert!(dest.to_string_lossy().contains("maps"));
    }
}
```

Run tests:
```bash
cargo test
```

## Debugging

### Enable Verbose Logging

```bash
RUST_LOG=debug cargo run --bin nexus
RUST_LOG=nexus_server=debug cargo run --bin nexus
RUST_LOG=nexus_downloader=debug cargo run --bin nexus
```

### Inspect Compilation

```bash
# Check without building
cargo check

# Verbose build output
cargo build -vv

# See generated code
cargo expand  # (requires `cargo-expand` crate)
```

### Use Debugger (VS Code)

Create `.vscode/launch.json`:
```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug nexus",
      "cargo": {
        "args": [
          "build",
          "--bin=nexus",
          "--package=server"
        ],
        "filter": {
          "name": "nexus",
          "kind": "bin"
        }
      },
      "args": [],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

Then press `F5` in VS Code to start debugging.

## Code Style & Conventions

### Naming
- **Modules**: lowercase, underscores (e.g., `download.rs`)
- **Types**: PascalCase (e.g., `DownloadTask`)
- **Functions**: snake_case (e.g., `create_download`)
- **Constants**: UPPER_SNAKE_CASE (e.g., `DEFAULT_PORT`)

### Documentation
- All public types must have doc comments: `/// Description`
- Document complex functions with examples
- Link related documentation in doc comments

```rust
/// File routing logic for content ingestion
///
/// Routes downloaded files to appropriate directories based on file type.
///
/// # Example
/// ```
/// let route = ContentRouter::route_file(
///     Path::new("map.pmtiles"),
///     Path::new("/data")
/// )?;
/// ```
pub struct ContentRouter;
```

### Code Organization
- Keep files focused (< 500 lines ideally)
- Use modules to organize related functions
- Import only what's needed

## Performance Profiling

### Build Time
```bash
# See which crates take longest to build
cargo build -j 1 -vv
```

### Runtime
```bash
# Use perf (Linux)
perf record cargo run --bin nexus
perf report

# Use Instruments (macOS)
cargo instruments -t "System Trace" --bin nexus
```

## Continuous Integration

### Local Testing Before Commit

```bash
# Run full test suite
cargo test --all

# Check formatting
cargo fmt --check

# Lint code
cargo clippy --all -- -D warnings

# Build release
cargo build --release

# Run binary
./target/release/nexus
```

### Git Hooks (Optional)

Create `.git/hooks/pre-commit`:
```bash
#!/bin/bash
cargo fmt --check && cargo clippy --all -- -D warnings
```

Make executable:
```bash
chmod +x .git/hooks/pre-commit
```

## Dependencies Management

### Viewing Dependencies

```bash
# Tree view
cargo tree

# Tree for specific crate
cargo tree -p server

# Outdated dependencies
cargo outdated
```

### Adding Dependencies

Add to workspace `Cargo.toml` `[workspace.dependencies]`:
```toml
my-crate = "1.0"
```

Then reference in individual crates:
```toml
my-crate.workspace = true
```

### Audit Security

```bash
# Check for known vulnerabilities
cargo audit

# Fix if possible
cargo audit fix
```

## Documentation Writing

### Markdown Style
- Use headings hierarchically (# → ## → ###)
- Code blocks with language: ` ```rust ` ` ` `
- Links to related docs: `[API_REFERENCE.md](./docs/API_REFERENCE.md)`
- Keep lines ≤ 100 characters

### Update Procedure
1. Make code changes
2. Update relevant docs in `docs/`
3. Update reference in [AGENTS.md](./AGENTS.md) if applicable
4. Test docs render correctly (GitHub, local preview)

## Troubleshooting

### Compilation Errors

**Error**: `unused variable` warning
```bash
# Fix automatically
cargo fix
```

**Error**: `dependency not found`
```bash
# Ensure workspace Cargo.toml has dependency
# Then run
cargo update
```

### Runtime Issues

**Server won't start**:
```bash
# Check if port is in use
netstat -tlnp | grep 8080

# Or use different port
PORT=8081 cargo run
```

**Data directory permission error**:
```bash
# Ensure data directory exists and is writable
mkdir -p data
chmod 755 data
```

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Axum Guide](https://docs.rs/axum/latest/axum/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Serde Documentation](https://serde.rs/)

## Getting Help

- **Documentation**: Check [docs/](./docs/) first
- **Code Examples**: See [crates/](./crates/) for implementations
- **GitHub Issues**: Search or create issue if stuck
- **Discussions**: Join GitHub Discussions for questions

---

**Last Updated**: 2026-07-17
**Maintained By**: Development Team
