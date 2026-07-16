# DEPLOYMENT.md — Deployment Guide

## Overview

Offline Nexus is designed for minimal-effort deployment across multiple environments.

## Single Binary (Recommended)

### Prerequisites
- Rust 1.70+ (for building) or pre-compiled binary from releases

### Build from Source

```bash
git clone https://github.com/yourusername/offline-nexus.git
cd offline-nexus
cargo build --release
```

**Output**: `target/release/nexus`

**Binary Size**: ~15-20 MB (depends on platform)

### Release Profile

Binary optimized for minimal size and maximum performance:

```toml
[profile.release]
opt-level = 3                # Maximum optimization
lto = "fat"                  # Link-time optimization
codegen-units = 1           # Single codegen unit (slower build, smaller binary)
strip = true                # Strip symbols
```

### Running

```bash
# Basic
./nexus

# Custom data directory
DATA_DIR=/mnt/usb/data ./nexus

# Custom port
PORT=8081 ./nexus

# With logging
RUST_LOG=debug ./nexus
```

### First Run

1. Binary creates `./data/` directory
2. Subdirectories created: `maps/`, `books/`, `poi/`, `inbox/`
3. Server starts on `http://localhost:8080`
4. Open browser and access UI

## Docker

### Building

```bash
docker build -t offline-nexus:latest .
```

**Dockerfile**:
```dockerfile
# Multi-stage build
FROM rust:1.75 as builder
WORKDIR /build
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /build/target/release/nexus /usr/local/bin/
EXPOSE 8080
ENV DATA_DIR=/data
VOLUME /data
CMD ["nexus"]
```

### Running

```bash
# Create data volume (persistent)
docker volume create nexus-data

# Run container
docker run -d \
  --name nexus \
  -p 8080:8080 \
  -v nexus-data:/data \
  offline-nexus:latest

# Access UI
open http://localhost:8080
```

### Docker Compose

```yaml
version: '3.8'

services:
  nexus:
    image: offline-nexus:latest
    ports:
      - "8080:8080"
    volumes:
      - nexus-data:/data
    environment:
      DATA_DIR: /data
      RUST_LOG: info
    restart: unless-stopped

volumes:
  nexus-data:
```

Run with:
```bash
docker-compose up -d
```

### Logs

```bash
docker logs nexus
docker logs -f nexus  # Follow logs
```

## Pre-compiled Binaries

Pre-compiled binaries available for all major platforms:

### Linux x86_64
```bash
curl -LO https://github.com/yourusername/offline-nexus/releases/download/v0.1.0/nexus-x86_64-linux
chmod +x nexus-x86_64-linux
./nexus-x86_64-linux
```

### Linux ARM64 (Raspberry Pi)
```bash
curl -LO https://github.com/yourusername/offline-nexus/releases/download/v0.1.0/nexus-aarch64-linux
chmod +x nexus-aarch64-linux
./nexus-aarch64-linux
```

### macOS
```bash
curl -LO https://github.com/yourusername/offline-nexus/releases/download/v0.1.0/nexus-x86_64-macos
chmod +x nexus-x86_64-macos
./nexus-x86_64-macos
```

### Windows
Download `nexus-x86_64-windows.exe` from releases, then:
```bash
./nexus-x86_64-windows.exe
```

## Raspberry Pi / ARM64 Setup

### System Requirements
- Raspberry Pi 3B+ or newer (2GB+ RAM recommended)
- 4GB microSD card (8GB+ for content)
- Power supply (5V, 2A+)

### Installation

1. **Download binary**:
   ```bash
   wget https://github.com/yourusername/offline-nexus/releases/download/v0.1.0/nexus-aarch64-linux
   chmod +x nexus-aarch64-linux
   ```

2. **Create data directory**:
   ```bash
   mkdir -p ~/nexus-data
   cd ~/nexus-data
   ```

3. **Run binary**:
   ```bash
   DATA_DIR=/home/pi/nexus-data /home/pi/nexus-aarch64-linux
   ```

4. **Access from another device**:
   ```bash
   # From laptop/phone
   curl http://<pi-ip>:8080
   open http://<pi-ip>:8080
   ```

### Systemd Service (Optional)

Create `/etc/systemd/system/nexus.service`:

```ini
[Unit]
Description=Offline Nexus - Off-Grid Content Platform
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pi
WorkingDirectory=/home/pi/nexus-data
ExecStart=/home/pi/nexus-aarch64-linux
Restart=on-failure
RestartSec=5s
Environment="DATA_DIR=/home/pi/nexus-data"
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable nexus
sudo systemctl start nexus
sudo systemctl status nexus
```

View logs:
```bash
sudo journalctl -u nexus -f
```

## Cross-Compilation

Build for different targets from any platform:

### Setup
```bash
rustup target add x86_64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-apple-darwin
```

### Build for Linux x86_64
```bash
cargo build --release --target x86_64-unknown-linux-gnu
```

### Build for Linux ARM64
```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

### Build for Windows
```bash
cargo build --release --target x86_64-pc-windows-gnu
```

### Build for macOS
```bash
cargo build --release --target x86_64-apple-darwin
```

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `DATA_DIR` | `./data` | Content storage directory |
| `PORT` | `8080` | Server port |
| `HOST` | `127.0.0.1` | Server hostname |
| `RUST_LOG` | `info` | Logging level |

## Network Configuration

### Local Network Access (Raspberry Pi)

To access from other devices on network:

1. Find Pi's IP:
   ```bash
   hostname -I
   # Example: 192.168.1.100
   ```

2. Access from laptop:
   ```bash
   open http://192.168.1.100:8080
   ```

### Port Forwarding (Internet Access)

⚠️ **Warning**: Not recommended for security reasons. Use VPN instead.

If needed:
```bash
# Forward external port 8080 to internal Pi:8080
iptables -t nat -A PREROUTING -p tcp --dport 8080 -j REDIRECT --to-port 8080
```

Better approach: Use WireGuard or Tailscale VPN.

## Storage Configuration

### External Drive (Raspberry Pi)

Mount USB drive for additional storage:

```bash
# Check connected drives
lsblk

# Format drive (if needed)
sudo mkfs.ext4 /dev/sda1

# Create mount point
sudo mkdir -p /mnt/nexus-data

# Mount drive
sudo mount /dev/sda1 /mnt/nexus-data

# Set permissions
sudo chown pi:pi /mnt/nexus-data

# Run with external drive
DATA_DIR=/mnt/nexus-data ./nexus-aarch64-linux
```

Auto-mount on boot (optional):
```bash
# Get UUID
blkid /dev/sda1

# Edit /etc/fstab
sudo nano /etc/fstab

# Add line:
# UUID=<your-uuid> /mnt/nexus-data ext4 defaults,auto 0 0
```

## Backup & Recovery

### Backing Up Data

```bash
# Backup entire data directory
tar -czf nexus-backup-$(date +%Y%m%d).tar.gz data/

# Backup to external drive
tar -czf /mnt/backup/nexus-backup-$(date +%Y%m%d).tar.gz data/
```

### Restoring Data

```bash
# Stop server
killall nexus

# Extract backup
tar -xzf nexus-backup-20260717.tar.gz

# Restart server
./nexus
```

## Troubleshooting

### Port Already in Use

```bash
# Check what's using port 8080
lsof -i :8080

# Change port
PORT=8081 ./nexus
```

### Data Directory Permission Error

```bash
# Check permissions
ls -la data/

# Fix ownership (if needed)
chown -R $(whoami):$(whoami) data/
```

### Server Won't Start

```bash
# Check logs with debug output
RUST_LOG=debug ./nexus

# Common issues:
# - Data directory doesn't exist (auto-created)
# - Port already in use
# - Corrupted data files (delete and restart)
```

### Can't Download Files

```bash
# Check internet connectivity
ping 8.8.8.8

# Check firewall
sudo ufw status

# If blocked:
sudo ufw allow 8080
```

### Memory Issues on Raspberry Pi

Monitor memory usage:
```bash
free -h
watch -n 1 free -h
```

If memory low:
- Reduce number of concurrent downloads (config, v0.2)
- Restart server periodically
- Upgrade to Raspberry Pi 4 (4GB+ RAM)

## Performance Tuning

### Binary Size Optimization

Already optimized with LTO and stripping. For additional reduction:

```toml
[profile.release]
opt-level = "z"      # Optimize for size
lto = "fat"
codegen-units = 1
strip = true
```

Result: ~12-15 MB (trade-off: slightly slower)

### Memory Usage

Typical memory usage:
- Idle: 10-20 MB
- With 10 maps loaded: 30-50 MB
- Downloading: +5 MB per concurrent download

### Disk I/O

- Content discovery: ~100 ms per 1000 files
- Download speed: Limited by network (not CPU-bound)

## Monitoring

### Health Check

```bash
# Check if server is running
curl http://localhost:8080/api/status

# Expected response:
# {"version":"0.1.0","status":"running",...}
```

### Metrics (Planned v0.2)

- Download speed (bytes/sec)
- Active task count
- CPU/memory usage
- Disk usage

## Updating

### From Source

```bash
cd offline-nexus
git pull
cargo build --release
./target/release/nexus
```

### From Binary

```bash
# Download new version
wget https://github.com/yourusername/offline-nexus/releases/download/v0.2.0/nexus-aarch64-linux

# Stop old version
killall nexus-aarch64-linux

# Run new version
./nexus-aarch64-linux
```

Data persists automatically; no migration needed.

---

**See Also**: [DEV_SETUP.md](./DEV_SETUP.md), [README.md](../README.md)
