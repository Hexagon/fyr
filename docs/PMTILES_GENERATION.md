# PMTiles Generation & Upload Guide

This guide explains how to create, optimize, and deploy **PMTiles** (Protocol Buffers Mobile Tiles) for use with Offline Nexus map viewer.

PMTiles is a modern, cloud-optimized tile format that dramatically reduces file size and improves performance compared to traditional XYZ tile sets. A country-sized map that would normally require thousands of image tiles can fit in a single optimized PMTiles file.

---

## What Are PMTiles?

### Overview

**PMTiles** = Single-file tile archive format for geospatial data

**Advantages**:
- **Single file**: No directory with thousands of PNG/JPEG tiles
- **Cloud-optimized**: Read-friendly headers enable fast random access
- **Compressed**: Dramatically smaller than traditional XYZ tile sets
- **Self-contained**: Includes metadata, attribution, min/max zoom
- **Flexible**: Vector tiles, raster tiles, or mixed

**Example sizes**:
- Traditional XYZ (OSM, zoom 1-13, whole world): ~500 GB tiles + metadata
- PMTiles (OSM, zoom 1-13, whole world): ~50-100 GB compressed PMTiles file
- PMTiles (Regional, e.g., Germany zoom 1-17): ~2-5 GB single file

### Supported Formats

- **Vector tiles** (MVT/Mapbox Vector Tiles) — Map styling with layers
- **Raster tiles** (PNG/JPEG) — Pre-rendered image tiles
- **Hybrid** — Mix of both in one archive

---

## Installation: PMTiles CLI

### Option 1: Pre-Built Binaries (Recommended)

Download from: https://github.com/protomaps/PMTiles/releases

```bash
# macOS
wget https://github.com/protomaps/PMTiles/releases/download/v3.13.0/pmtiles-macos
chmod +x pmtiles-macos
./pmtiles-macos --version

# Linux (x86_64)
wget https://github.com/protomaps/PMTiles/releases/download/v3.13.0/pmtiles-linux
chmod +x pmtiles-linux
./pmtiles-linux --version

# Windows
# Download: https://github.com/protomaps/PMTiles/releases/download/v3.13.0/pmtiles-windows.exe
pmtiles-windows.exe --version

# Raspberry Pi (ARM64)
wget https://github.com/protomaps/PMTiles/releases/download/v3.13.0/pmtiles-linux-arm64
chmod +x pmtiles-linux-arm64
./pmtiles-linux-arm64 --version
```

### Option 2: Build from Source

```bash
git clone https://github.com/protomaps/PMTiles.git
cd PMTiles
go build -o pmtiles ./cmd/pmtiles
```

### Option 3: Via Package Manager

```bash
# macOS (Homebrew)
brew install pmtiles

# Arch Linux
pacman -S pmtiles

# Ubuntu (from source or PPA, check distribution)
```

---

## Tile Sources

### Source 1: OpenStreetMap (Recommended for Offline Nexus)

**Free, global, CC-BY-SA licensed**

#### Option A: Pre-Built OSM PMTiles

Protomaps maintains pre-built, optimized OSM PMTiles:

```bash
# Download global map (zoom 0-14, optimized, ~280 MB)
wget https://maps.protomaps.com/downloads/pmtiles/2024_01_v3_planet.pmtiles

# Or regional extracts (faster download)
# Europe, North America, Asia, South America, Africa, Oceania
wget https://maps.protomaps.com/downloads/pmtiles/2024_01_v3_europe.pmtiles

# Move to Offline Nexus data directory
mv 2024_01_v3_europe.pmtiles data/maps/europe-osm.pmtiles
```

**Recommended for**: Most deployments. Professional quality, optimized for performance.

#### Option B: Export from Overpass API

Extract specific regions or features from OSM:

```bash
# Example: Download all buildings in Berlin
wget -O berlin_buildings.osm \
  'https://overpass-api.de/api/interpreter?data=[bbox:52.3,13.0,52.7,13.8];(way["building"];relation["building"];);out body;'

# Convert OSM to MBTiles (intermediate format)
tippecanoe -o berlin_buildings.mbtiles berlin_buildings.osm

# Convert MBTiles to PMTiles
pmtiles convert berlin_buildings.mbtiles berlin_buildings.pmtiles

# Place in Offline Nexus
mv berlin_buildings.pmtiles data/maps/
```

### Source 2: Bing Maps / Satellite Imagery

**Commercial, free tier available, high-resolution imagery**

```bash
# Download Bing satellite imagery for region
# Tools: QGIS, GDAL, or tile-specific tools

# Convert to PMTiles
gdal_translate -of COG bing_satellite.tif bing_satellite.mbtiles
pmtiles convert bing_satellite.mbtiles bing_satellite.pmtiles

# Note: Verify Bing Maps terms of service for offline use
```

### Source 3: USGS, NOAA, or Gov Data

Various government agencies provide geospatial data:

```bash
# Example: USGS topographic data
# Download: https://www.usgs.gov/faqs/what-are-geographic-data-files

# Convert to PMTiles following similar GDAL workflow
gdal_translate -of COG usgs_topo.tif usgs_topo.pmtiles
```

---

## Generate PMTiles from Vector Data

### Workflow: GeoJSON → Vector Tiles → PMTiles

#### Step 1: Prepare GeoJSON

```bash
# Example: City POIs (restaurants, hospitals, etc.)
cat > pois.geojson << 'EOF'
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "geometry": {"type": "Point", "coordinates": [13.405, 52.520]},
      "properties": {"name": "Restaurant A", "category": "food"}
    },
    {
      "type": "Feature",
      "geometry": {"type": "Point", "coordinates": [13.406, 52.521]},
      "properties": {"name": "Hospital B", "category": "medical"}
    }
  ]
}
EOF
```

#### Step 2: Convert to Vector Tiles (MBTiles)

```bash
# Install tippecanoe (Mapbox tool for GeoJSON → tiles)
# macOS: brew install tippecanoe
# Linux: build from https://github.com/mapbox/tippecanoe

tippecanoe \
  -o pois.mbtiles \
  --drop-densest-as-needed \
  --extend-zoom-max=16 \
  pois.geojson

# Options explained:
# --drop-densest-as-needed: Reduce detail at lower zoom levels
# --extend-zoom-max=16: Generate tiles up to zoom 16
```

#### Step 3: Convert to PMTiles

```bash
pmtiles convert pois.mbtiles pois.pmtiles

# Verify
pmtiles info pois.pmtiles
# Output:
#   Zoom levels: 0-16
#   Tile count: 12,345
#   Size: ~5 MB
```

#### Step 4: Move to Offline Nexus

```bash
mv pois.pmtiles data/maps/pois.pmtiles
```

---

## Generate PMTiles from Raster Data

### Workflow: GeoTIFF/PNG → Raster Tiles → PMTiles

#### Step 1: Prepare Georeferenced Image

```bash
# Example: Satellite or map scan with geographic coordinates

# Verify georeference with GDAL
gdalinfo satellite_image.tif | grep -A 5 "Geotransform"
```

#### Step 2: Create Raster Tile Pyramid

```bash
# Build pyramid with GDAL (creates internal tiles)
gdaladdo -r average satellite_image.tif 2 4 8 16 32

# Export as COG (Cloud Optimized GeoTIFF)
gdal_translate -of COG satellite_image.tif satellite_cog.tif
```

#### Step 3: Convert to PMTiles

```bash
# Create PMTiles from raster
gdal_translate \
  -of PMTiles \
  -co TILED=YES \
  -co ZOOM_LEVEL=12 \
  satellite_cog.tif \
  satellite_map.pmtiles

# Verify
pmtiles info satellite_map.pmtiles
```

---

## Optimization & Tuning

### File Size Optimization

```bash
# Check current file size
ls -lh data/maps/my-map.pmtiles

# Analyze details
pmtiles info data/maps/my-map.pmtiles

# If too large, re-tile with compression
pmtiles convert \
  --compress=zstd \
  --compress-level=22 \
  my-map.mbtiles \
  my-map-compressed.pmtiles
```

### Zoom Level Strategy

```bash
# For offline deployment, consider zoom levels:

# Minimal (fast download, local use): 0-12
# - Covers entire world with sufficient detail for navigation
# - File size: 50-200 MB

# Regional (country/state): 0-15
# - Street level detail
# - File size: 500 MB - 2 GB

# Detailed (city/county): 0-17
# - Individual building visibility
# - File size: 2-10 GB

# Example: Build with limited zoom
tippecanoe \
  -o city-map.mbtiles \
  --minimum-zoom=10 \
  --maximum-zoom=17 \
  city-data.geojson

pmtiles convert city-map.mbtiles city-map.pmtiles
```

### Compression

```bash
# Default compression (snappy)
pmtiles convert input.mbtiles output.pmtiles

# Max compression (slower encode, smaller file)
pmtiles convert --compress=zstd --compress-level=22 input.mbtiles output.pmtiles

# No compression (fastest, larger file)
pmtiles convert --compress=none input.mbtiles output.pmtiles
```

---

## Deployment with Offline Nexus

### Step 1: Place PMTiles in Data Directory

```bash
# Organize by region/purpose
data/maps/
├── world-osm.pmtiles          # Global OSM
├── europe-osm.pmtiles         # Regional OSM
├── germany-satellite.pmtiles   # Satellite imagery
└── pois-medical.pmtiles        # Medical facilities
```

### Step 2: Verify File Routing

Offline Nexus automatically detects PMTiles files:

```bash
# File detected when:
# - Located in data/maps/
# - Has .pmtiles extension
# - ContentRouter.route_file() maps to ContentType::Map

# Verify in API:
curl http://localhost:8080/api/content/maps
# Response includes all .pmtiles files
```

### Step 3: Access in UI

Browser-based map viewer loads PMTiles via [Leaflet](https://leafletjs.com) + [@maplibre/pmtiles](https://www.npmjs.com/package/@maplibre/pmtiles):

```javascript
// Example from Offline Nexus Vue/React frontend
import { Protocol } from "@maplibre/pmtiles";

const protocol = new Protocol();
maplibregl.addProtocol("pmtiles", protocol.tile);

const map = new maplibregl.Map({
  container: "map",
  source: {
    type: "vector",
    url: "pmtiles:///api/content/maps/europe-osm.pmtiles"
  },
  center: [10, 50],
  zoom: 4
});
```

---

## Advanced: Continuous Updates

### Scenario: Keep Map Data Current

```bash
# Automated weekly update script
#!/bin/bash

BACKUPS_DIR="data/maps/backups"
CURRENT_FILE="data/maps/osm-current.pmtiles"

# Backup current version
cp $CURRENT_FILE "$BACKUPS_DIR/osm-$(date +%Y%m%d).pmtiles"

# Download latest
wget -O /tmp/osm-latest.pmtiles \
  https://maps.protomaps.com/downloads/pmtiles/latest.pmtiles

# Verify (optional)
pmtiles info /tmp/osm-latest.pmtiles > /dev/null && \
  mv /tmp/osm-latest.pmtiles $CURRENT_FILE

# Clean old backups (keep last 4)
ls -t "$BACKUPS_DIR"/* | tail -n +5 | xargs rm
```

### Peer-to-Peer Sync (v1.0+)

Future Offline Nexus will support P2P map sync:

```bash
# Planned for v1.0
# Sync latest PMTiles between Offline Nexus instances
# without central server

nexus sync --peer=192.168.1.100 --file=osm-current.pmtiles
```

---

## Troubleshooting

### Issue: PMTiles file won't load in browser

**Debug**:
```bash
# Verify file integrity
pmtiles info data/maps/problem.pmtiles

# Check API endpoint
curl http://localhost:8080/api/content/maps

# Check file permissions
ls -l data/maps/problem.pmtiles
chmod 644 data/maps/problem.pmtiles
```

### Issue: Very large file size (>5 GB)

**Solutions**:
```bash
# Reduce zoom levels
pmtiles convert --min-zoom=0 --max-zoom=14 large.mbtiles smaller.pmtiles

# Use compression
pmtiles convert --compress=zstd --compress-level=22 large.mbtiles smaller.pmtiles

# Use simpler data source
# (Switch from full satellite imagery to vector-only OSM)
```

### Issue: Slow tile rendering

**Optimization**:
```bash
# Ensure file is on fast storage (SSD preferred)
# Reduce zoom levels for faster initial load
# Preload tiles at common zoom levels

# Check server logs for errors
journalctl -u offline-nexus -f
```

---

## Resources

- **PMTiles Spec**: https://protomaps.com/docs/pmtiles/
- **PMTiles Tools**: https://github.com/protomaps/PMTiles
- **Tippecanoe**: https://github.com/mapbox/tippecanoe (GeoJSON → tiles)
- **GDAL/QGIS**: https://gdal.org (Raster/vector conversions)
- **Overpass API**: https://overpass-turbo.eu (OSM data extraction)
- **Protomaps Downloads**: https://protomaps.com/downloads

---

## See Also

- [WIKIPEDIA_SUPPORT.md](./WIKIPEDIA_SUPPORT.md) — Offline Wikipedia integration
- [DEVELOPER_DEPLOYMENT.md](./DEVELOPER_DEPLOYMENT.md) — Deploy on various platforms
- [API_REFERENCE.md](./API_REFERENCE.md) — Content endpoints
