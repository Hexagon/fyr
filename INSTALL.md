# Fyr Installation Guide

This guide is the authoritative reference for deploying Fyr. Choose one of three installation paths:

* [Docker](#docker) — run on any existing Linux, macOS, or Windows machine.
* [Raspberry Pi OS From Scratch](#raspberry-pi-os-from-scratch) — clean Raspberry Pi deployments.
* [From Source](#from-source-development) — build locally for development workflows.

If you only need a quick local launch, see the One-Minute Start in [README.md](README.md).

---

## Docker

Use this path for a fast, repeatable deployment on Linux, macOS, or Windows.

Fyr stores all user data in `DATA_DIR` (`/data` in Docker examples). Always mount a persistent volume or bind-mount so data survives container replacement.

### Run (named volume — recommended)

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

### Run (host folder bind-mount)

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v /path/to/fyr-data:/data \
  hexagon/fyr:latest
```

Windows PowerShell bind-mount:

```powershell
docker run --rm -p 8080:8080 `
  -e FYR_HOST=0.0.0.0 `
  -e DATA_DIR=/data `
  -v C:\fyr-data:/data `
  hexagon/fyr:latest
```

Open `http://localhost:8080` on the same machine, or `http://<host-or-device-ip>:8080` from another device.

### Bind-Mount Permissions (Linux)

The Fyr container runs as UID 1000. If your host folder is owned by a different user, the container will fail to write data. Fix this by setting the correct ownership before starting Fyr:

```bash
mkdir -p /path/to/fyr-data
sudo chown -R 1000:1000 /path/to/fyr-data
```

Then use that folder as the bind-mount target. Named volumes (managed by Docker) do not have this issue.

### Dev Image

Use `hexagon/fyr:dev` for testing pre-release builds. Replace `:latest` with `:dev` in any command above. Do not use the dev image for production deployments.

### Upgrading

1. Pull the new image:

```bash
docker pull hexagon/fyr:latest
```

2. Stop and remove the running container:

```bash
docker stop <container-id-or-name>
docker rm <container-id-or-name>
```

3. Start a new container with the same `-v` mount and environment variables. Your data volume is untouched.

---

## Raspberry Pi OS From Scratch

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

### 3) Run Fyr

Fyr images support `linux/arm64`, which matches Raspberry Pi 64-bit OS.

```bash
docker run --rm -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

Open from another device on the same network: `http://<raspberry-pi-ip>:8080`

### 4) Optional: Start Automatically on Boot

```bash
docker run -d --restart unless-stopped --name fyr \
  -p 8080:8080 \
  -e FYR_HOST=0.0.0.0 \
  -e DATA_DIR=/data \
  -v fyr-data:/data \
  hexagon/fyr:latest
```

### Bind-Mount Permissions

If you use a host folder bind-mount instead of a named volume, set ownership to UID 1000 first (see [Bind-Mount Permissions](#bind-mount-permissions-linux) above).

### Upgrading

1. Pull the new image:

```bash
docker pull hexagon/fyr:latest
```

2. If running with `--restart unless-stopped`, stop and remove the named container:

```bash
docker stop fyr
docker rm fyr
```

3. Re-run the same `docker run` command from step 3 or 4 above. The `fyr-data` volume is preserved.

---

## From Source (Development)

Use this path when you want to develop Fyr or run local code changes.

### Prerequisites

* Rust stable toolchain
* Node.js 24 (recommended to match CI)
* npm

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

4. Open `http://localhost:8080` on the same machine, or `http://<host-or-device-ip>:8080` from another device.

### Optional Runtime Overrides

* `DATA_DIR` (default `./public/data`)
* `FYR_HOST` (default `127.0.0.1`; use `0.0.0.0` for LAN access)
* `FYR_PORT` (default `8080`)

### Upgrading

Pull the latest changes and rebuild:

```bash
git pull
cd crates/ui/frontend
npm ci
npm run build
cd ../../..
cargo build --release -p server --bin fyr
```

Restart the binary. Your data directory is separate from the build output, so no data is affected.

---

## Quick Verification Checklist

* `docker ps` (if running in Docker) shows the Fyr container as running.
* Browser can open `/api/status` on your target host and port.
* `DATA_DIR` location is writable.
* Port `8080` is not blocked by a firewall or already in use.

---

## Troubleshooting

For common startup issues, write permission errors, missing content after a restart, or port conflicts, see the **Troubleshooting** section in the [User Manual](docs/user/USER_MANUAL.md).

---

## Related Documentation

* User manual: [docs/user/USER_MANUAL.md](docs/user/USER_MANUAL.md)
* Developer manual: [docs/developer/DEVELOPER_MANUAL.md](docs/developer/DEVELOPER_MANUAL.md)