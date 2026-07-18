# Fyr Installation Guide

This guide provides three installation paths:

- Option A: build from source for development workflows.
- Option B: run with Docker on an existing system.
- Option C: install Raspberry Pi OS from scratch, then run Fyr with Docker.

If you only need a quick local launch, see [README.md](README.md).

## Option A: From Source (Development)

Use this path when you want to develop Fyr or run local code changes.

### Prerequisites

- Rust stable toolchain
- Node.js 24 (recommended to match CI)
- npm

### Build and Run

1. Build frontend assets:

```bash
cd crates/ui/frontend
npm ci
npm run build
cd ../../..
```

2. Build backend binary:

```bash
cargo build --release -p server --bin fyr
```

3. Run Fyr:

```bash
./target/release/fyr
```

Windows PowerShell:

```powershell
.\target\release\fyr.exe
```

4. Open: http://127.0.0.1:8080

### Optional Runtime Overrides

- `DATA_DIR` (default `./public/data`)
- `FYR_HOST` (default `127.0.0.1`)
- `FYR_PORT` (default `8080`)

## Option B: Docker on an Existing System

Use this path for a fast, repeatable deployment on Linux, macOS, or Windows.

### Production Image

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

### Dev Image

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:dev
```

Open: http://127.0.0.1:8080

### Notes

- Keep `-v fyr-data:/data` to persist maps, books, models, and downloads.
- `hexagon/fyr:dev` is for testing and validation; use `hexagon/fyr:latest` for production.

## Option C: Raspberry Pi OS From Scratch (with Docker)

Use this path for clean Raspberry Pi deployments.

### 1) Install Raspberry Pi OS

1. Flash Raspberry Pi OS 64-bit (Bookworm recommended).
2. Boot and update packages:

```bash
sudo apt update
sudo apt full-upgrade -y
sudo reboot
```

After reboot:

```bash
sudo apt update
```

### 2) Install Docker

```bash
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
newgrp docker
docker --version
```

### 3) Run Fyr in Docker on Raspberry Pi

Fyr images support `linux/arm64`, which matches Raspberry Pi 64-bit OS.

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

Open from another device on the same network:

- `http://<raspberry-pi-ip>:8080`

### 4) Optional: Start Automatically on Boot

```bash
docker run -d --restart unless-stopped --name fyr \
  -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

## Quick Verification Checklist

- `docker ps` (if running in Docker) shows Fyr container as running.
- Browser can open `/api/status` on your target host and port.
- `DATA_DIR` location is writable.
- Port `8080` is not blocked by firewall or already occupied.

## Troubleshooting

- Port in use: change `FYR_PORT` and host port mapping (for Docker).
- Permission errors: ensure the selected `DATA_DIR` path is writable.
- Missing content after restart: confirm the `/data` volume is mounted.

## Related Documentation

- User manual: [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
- Developer manual: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)
- Project governance and documentation policy: [AGENTS.md](AGENTS.md)