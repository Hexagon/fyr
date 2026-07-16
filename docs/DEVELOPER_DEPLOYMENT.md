# Developer Deployment Guide

This guide covers deploying Offline Nexus in production environments for developers, system administrators, and DevOps teams.

---

## Quick Start: Local Development

### Prerequisites

- Rust 1.70+
- Git
- ~500 MB disk space for initial build and minimal data

### Build & Run

```bash
git clone https://github.com/Hexagon/offline-nexus.git
cd offline-nexus

# Build optimized binary
cargo build --release
# Output: target/release/nexus (~1.54 MB)

# Run with default settings
./target/release/nexus
# Output: Starting on http://127.0.0.1:8080
```

### Configuration

**Default settings**:
- **Host**: 127.0.0.1 (localhost only, v0.1)
- **Port**: 8080
- **Data directory**: ./data/ (auto-created)

**Access UI**: http://localhost:8080

---

## Deployment: Linux Server (Systemd)

### Prerequisites

- Linux server (Ubuntu 20.04+, Debian 11+, CentOS 8+, etc.)
- Systemd init system
- ~2-50 GB disk space (depends on content)

### Step 1: Download or Build Binary

#### Option A: Download Pre-Built Binary

```bash
# Create application directory
sudo mkdir -p /opt/offline-nexus
cd /opt/offline-nexus

# Download latest binary
RELEASE_URL="https://github.com/Hexagon/offline-nexus/releases/download/v0.1.0/nexus-x86_64-linux"
sudo wget -O nexus "$RELEASE_URL"
sudo chmod +x nexus

# Verify
./nexus --version 2>&1 | head -1
```

#### Option B: Build from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Clone and build
git clone https://github.com/Hexagon/offline-nexus.git
cd offline-nexus
cargo build --release

# Move binary to system location
sudo cp target/release/nexus /opt/offline-nexus/nexus
sudo chmod +x /opt/offline-nexus/nexus
```

### Step 2: Create Data Directory

```bash
# Create data directory with proper permissions
sudo mkdir -p /var/lib/offline-nexus/data
sudo chown -R nexus:nexus /var/lib/offline-nexus
sudo chmod 755 /var/lib/offline-nexus
sudo chmod 755 /var/lib/offline-nexus/data
```

### Step 3: Create Systemd Service

Create `/etc/systemd/system/offline-nexus.service`:

```ini
[Unit]
Description=Offline Nexus - Off-Grid Content Platform
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=nexus
Group=nexus
WorkingDirectory=/var/lib/offline-nexus
ExecStart=/opt/offline-nexus/nexus

# Environment
Environment="RUST_LOG=info"
Environment="DATA_DIR=/var/lib/offline-nexus/data"
Environment="NEXUS_HOST=0.0.0.0"
Environment="NEXUS_PORT=8080"

# Process management
Restart=on-failure
RestartSec=5s
KillMode=process

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=offline-nexus

# Security (optional, hardening)
PrivateTmp=yes
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

### Step 4: Create System User

```bash
# Create unprivileged 'nexus' user (if not exists)
sudo useradd -r -s /bin/false nexus 2>/dev/null || true

# Verify
id nexus
```

### Step 5: Start Service

```bash
# Reload systemd configuration
sudo systemctl daemon-reload

# Start service
sudo systemctl start offline-nexus

# Enable auto-start on boot
sudo systemctl enable offline-nexus

# Verify running
sudo systemctl status offline-nexus

# Expected output:
# ● offline-nexus.service - Offline Nexus - Off-Grid Content Platform
#    Loaded: loaded (/etc/systemd/system/offline-nexus.service; enabled; ...)
#    Active: active (running)
```

### Step 6: Configure Reverse Proxy (Optional)

Nginx reverse proxy for external access:

```nginx
# /etc/nginx/sites-available/offline-nexus

upstream nexus_backend {
    server 127.0.0.1:8080;
}

server {
    listen 80;
    server_name offline-nexus.example.com;

    # Redirect HTTP to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name offline-nexus.example.com;

    ssl_certificate /etc/letsencrypt/live/offline-nexus.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/offline-nexus.example.com/privkey.pem;

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;

    location / {
        proxy_pass http://nexus_backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # Enable compression for large responses
        proxy_buffering on;
        proxy_buffer_size 128k;
        proxy_buffers 4 128k;
    }
}
```

Enable site:

```bash
sudo ln -s /etc/nginx/sites-available/offline-nexus \
           /etc/nginx/sites-enabled/

sudo nginx -t
sudo systemctl restart nginx
```

---

## Deployment: Docker

### Option 1: Pre-Built Docker Image

```bash
# Pull from registry (when available)
docker pull offline-nexus:latest

# Run container
docker run \
  -d \
  --name offline-nexus \
  -p 8080:8080 \
  -v /data/offline-nexus:/data \
  -e RUST_LOG=info \
  offline-nexus:latest

# Verify running
docker logs -f offline-nexus
```

### Option 2: Build Your Own Image

```bash
# From project root
docker build -t offline-nexus:local .

# Run
docker run \
  -d \
  --name offline-nexus \
  -p 8080:8080 \
  -v $(pwd)/data:/data \
  offline-nexus:local
```

### Docker Compose (Multi-Container Setup)

```yaml
# docker-compose.yml
version: '3.8'

services:
  offline-nexus:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: offline-nexus
    ports:
      - "8080:8080"
    volumes:
      - nexus-data:/data
      - ./config:/config:ro
    environment:
      - RUST_LOG=info
      - NEXUS_HOST=0.0.0.0
      - NEXUS_PORT=8080
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/api/status"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s

  nginx-proxy:
    image: nginx:alpine
    container_name: offline-nexus-proxy
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - ./ssl:/etc/nginx/ssl:ro
    depends_on:
      - offline-nexus
    restart: unless-stopped

volumes:
  nexus-data:
    driver: local
```

Run with: `docker-compose up -d`

---

## Deployment: Raspberry Pi (ARM64)

### Prerequisites

- Raspberry Pi 4 (4GB+ RAM recommended)
- Ubuntu 22.04 LTS (ARM64) or Raspberry Pi OS (64-bit)
- 16 GB microSD or larger
- External USB drive for content (optional, 100+ GB recommended)

### Installation

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install dependencies
sudo apt install -y curl git

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Add ARM64 target (if needed)
rustup target add aarch64-unknown-linux-gnu

# Clone and build
git clone https://github.com/Hexagon/offline-nexus.git
cd offline-nexus

# Build (ARM64, may take 5-10 minutes)
cargo build --release --target aarch64-unknown-linux-gnu

# Install
sudo cp target/aarch64-unknown-linux-gnu/release/nexus /usr/local/bin/
sudo chmod +x /usr/local/bin/nexus
```

### Configure for External USB Storage

```bash
# Mount USB drive
sudo mkdir -p /mnt/offline-nexus-data
sudo mount /dev/sda1 /mnt/offline-nexus-data

# Persist mount in /etc/fstab
echo "/dev/sda1 /mnt/offline-nexus-data ext4 defaults,auto 0 0" | sudo tee -a /etc/fstab

# Create data directory
sudo mkdir -p /mnt/offline-nexus-data/nexus-data
sudo chown -R pi:pi /mnt/offline-nexus-data/nexus-data
```

### Systemd Service (Raspberry Pi)

```ini
# /etc/systemd/system/offline-nexus.service

[Unit]
Description=Offline Nexus (ARM64)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pi
Group=pi
WorkingDirectory=/mnt/offline-nexus-data/nexus-data
ExecStart=/usr/local/bin/nexus

Environment="RUST_LOG=info"
Environment="DATA_DIR=/mnt/offline-nexus-data/nexus-data"
Environment="NEXUS_HOST=0.0.0.0"
Environment="NEXUS_PORT=8080"

Restart=on-failure
RestartSec=10s

StandardOutput=journal
StandardError=journal
SyslogIdentifier=nexus

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl start offline-nexus
sudo systemctl enable offline-nexus

# Monitor
sudo journalctl -u offline-nexus -f
```

### Local Network Access

Access from other devices:

```bash
# Find Raspberry Pi IP
hostname -I
# Output: 192.168.1.150 ...

# From another device on network:
# Open browser: http://192.168.1.150:8080
```

---

## Environment Variables

Configure Offline Nexus behavior via environment variables (roadmap for v0.2+):

```bash
# Logging
RUST_LOG=info|debug|trace|warn|error

# Server binding
NEXUS_HOST=0.0.0.0           # Listen on all interfaces
NEXUS_PORT=8080               # HTTP port

# Data directory
DATA_DIR=/var/lib/offline-nexus/data

# Performance tuning
NEXUS_MAX_CONTENT_SIZE=104857600  # 100 MB upload limit (v0.2+)
NEXUS_WORKER_THREADS=4            # Download worker threads (v0.2+)

# Feature flags (v0.2+)
NEXUS_ENABLE_SEARCH=true          # Full-text search on Wikipedia
NEXUS_ENABLE_P2P=false            # P2P sync (v1.0+)

# Example usage
export RUST_LOG=debug
export NEXUS_HOST=0.0.0.0
export NEXUS_PORT=9000
./nexus
```

---

## Monitoring & Logging

### Systemd Journaling

```bash
# View recent logs
sudo journalctl -u offline-nexus -n 50

# Follow logs in real-time
sudo journalctl -u offline-nexus -f

# Filter by log level
sudo journalctl -u offline-nexus -p err

# Last 1 hour
sudo journalctl -u offline-nexus --since "1 hour ago"
```

### Health Check Endpoint

```bash
# Check server status
curl http://localhost:8080/api/status

# Expected output:
{
  "version": "0.1.0",
  "status": "ok",
  "data_dir": "/var/lib/offline-nexus/data",
  "content_count": {
    "maps": 5,
    "books": 12,
    "poi": 3
  }
}
```

### Monitoring with Prometheus (Advanced)

```bash
# Add metrics endpoint (roadmap v0.2+)
curl http://localhost:8080/metrics

# Scrape config in prometheus.yml
scrape_configs:
  - job_name: 'offline-nexus'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
```

---

## Updating & Maintenance

### Update Binary

```bash
# Download latest release
cd /tmp
wget https://github.com/Hexagon/offline-nexus/releases/download/v0.1.1/nexus-x86_64-linux

# Backup current
sudo cp /opt/offline-nexus/nexus /opt/offline-nexus/nexus.bak

# Install new version
sudo mv nexus /opt/offline-nexus/nexus
sudo chmod +x /opt/offline-nexus/nexus

# Restart service
sudo systemctl restart offline-nexus

# Verify
sudo systemctl status offline-nexus
```

### Backup Content

```bash
# Backup data directory
tar -czf offline-nexus-backup-$(date +%Y%m%d).tar.gz \
  /var/lib/offline-nexus/data/

# Restore (if needed)
tar -xzf offline-nexus-backup-20260717.tar.gz -C /

# Offsite backup
rclone copy /var/lib/offline-nexus/data/ remote:/backup/nexus/
```

---

## Troubleshooting

### Issue: Port Already in Use

```bash
# Check what's using port 8080
sudo lsof -i :8080

# Kill process (if safe to do so)
sudo kill -9 <PID>

# Or use different port via environment
export NEXUS_PORT=9000
```

### Issue: Permission Denied on Data Directory

```bash
# Fix permissions
sudo chown -R nexus:nexus /var/lib/offline-nexus
sudo chmod -R 755 /var/lib/offline-nexus

# Restart service
sudo systemctl restart offline-nexus
```

### Issue: Service Won't Start

```bash
# Check systemd status
sudo systemctl status offline-nexus

# View detailed error logs
sudo journalctl -u offline-nexus -n 100

# Test binary manually
/opt/offline-nexus/nexus
# Run in foreground to see output
```

### Issue: Out of Disk Space

```bash
# Check available space
df -h

# Check data directory size
du -sh /var/lib/offline-nexus/data/

# Remove old backups or excess content
ls -lh /var/lib/offline-nexus/data/maps/
# Delete unused .pmtiles files
```

---

## Performance Tuning

### CPU & Memory

```bash
# Check resource usage
ps aux | grep nexus

# Monitor in real-time
top -p $(pgrep nexus)

# Increase available resources
# - Upgrade hardware (for Raspberry Pi)
# - Run on faster storage (SSD over microSD)
# - Reduce concurrent downloads (v0.2+)
```

### Disk I/O

```bash
# Use SSD for data directory
# Traditional HDD: 100+ random IOPS
# SSD: 10,000+ random IOPS

# Monitor I/O
iostat -x /dev/sda 1

# For Raspberry Pi: Consider external SSD via USB 3.0
```

### Network

```bash
# Check bandwidth usage
iftop

# Limit bandwidth (optional)
# Install wondershaper
sudo apt install wondershaper

# Limit to 100 Mbps down, 50 Mbps up
sudo wondershaper eth0 100 50
```

---

## Security Hardening (Roadmap v0.2)

```bash
# Run behind reverse proxy (recommended)
# See Nginx section above

# Use systemd sandboxing
# Add to service file:
# SystemCallFilter=~@clock @debug @module @mount @obsolete @privileged @reboot @swap
# RestrictNamespaces=yes

# Firewall rules (UFW)
sudo ufw allow 8080/tcp     # Local network only
sudo ufw allow 80/tcp       # HTTP for reverse proxy
sudo ufw allow 443/tcp      # HTTPS for reverse proxy
sudo ufw enable
```

---

## See Also

- [WIKIPEDIA_SUPPORT.md](./WIKIPEDIA_SUPPORT.md) — Offline Wikipedia content
- [PMTILES_GENERATION.md](./PMTILES_GENERATION.md) — Map data setup
- [DEPLOYMENT.md](./DEPLOYMENT.md) — User-focused deployment
- [DEV_SETUP.md](./DEV_SETUP.md) — Development environment
