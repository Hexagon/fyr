# DATA_FORMATS.md — Supported Content Formats

This document specifies all supported file formats, validation rules, and technical specifications for Offline Nexus content.

## Maps: PMTiles

### Overview
**PMTiles** is an efficient, cloud-optimized tile archive format that enables HTTP range requests for efficient map tile serving.

### Specification
- **Format Version**: PMTiles 3.0+
- **Container**: Binary archive format
- **File Extension**: `.pmtiles`
- **Typical Size**: 50 MB - 5 GB (depending on coverage)
- **Use Case**: Vector and raster map tiles with zoom levels

### File Structure
```
Header (7 bytes): "PMTiles"
Root Directory
Tile Data (gzip compressed)
Metadata
```

### Validation
**Rule**: Warn-only. File accepted if:
- File exists and is readable
- File size ≥ 7 bytes
- Header contains magic string "PMTiles"

**Warning Scenarios**:
- Header doesn't match → Log warning, accept anyway
- File very small (< 1 KB) → Likely invalid
- Read error → Log error, mark as incomplete

**Implementation**: See `nexus-core/src/validation.rs:validate_pmtiles()`

### Serving
- **Storage**: `data/maps/`
- **Access**: `/api/content/maps` endpoint
- **Client Viewer**: Leaflet/OpenLayers with @maplibre/pmtiles
- **Range Requests**: HTTP 206 for efficient tile delivery

### Example
```bash
# Download OpenStreetMap PMTiles
wget https://example.com/osm-europe.pmtiles
# Place in data/maps/
cp osm-europe.pmtiles data/maps/
# Accessible at: http://localhost:8080 (after UI loads)
```

### Tools & Resources
- **Creation**: `tippecanoe` (create from data)
- **Verification**: `pmtiles validate` command
- **Hosting**: Works with any HTTP server (Nexus included)

---

## Books: EPUB, PDF, MOBI

### EPUB (Recommended)
**Electronic Publication** — XML-based, widely compatible

- **Format Version**: EPUB 2.0, 3.0
- **File Extension**: `.epub`
- **Container**: ZIP file with OPF manifest
- **Typical Size**: 1 MB - 50 MB
- **Metadata**: Title, author, cover image included

**Structure**:
```
book.epub
├── mimetype (application/epub+zip)
├── META-INF/
│   ├── container.xml (points to OPF)
│   └── metadata.xml
├── OEBPS/
│   ├── content.opf (package manifest)
│   ├── toc.ncx (table of contents)
│   ├── chapters/
│   │   ├── chapter1.html
│   │   └── chapter2.html
│   ├── css/
│   └── images/
└── ...
```

**Validation**:
- ZIP structure check
- OPF file presence
- Metadata extraction

### PDF
**Portable Document Format** — Universal

- **Format Version**: PDF 1.4+
- **File Extension**: `.pdf`
- **Typical Size**: 0.5 MB - 100 MB
- **Advantages**: Fixed layout, universal support
- **Disadvantages**: Less flexible for small screens

**Validation**:
- PDF header check (%PDF)
- Stream integrity

### MOBI
**Mobipocket Format** — Kindle-compatible

- **Format Version**: MOBI 7+
- **File Extension**: `.mobi`
- **Typical Size**: 1 MB - 30 MB
- **Advantages**: Kindle ecosystem
- **Disadvantages**: Less flexible layout

**Validation**:
- MOBI header check
- Metadata extraction

### Storage & Serving
- **Storage**: `data/books/`
- **Subdirectories**: Supported (e.g., `data/books/fiction/`, `data/books/reference/`)
- **Access**: `/api/content/books` endpoint
- **Client Viewer**: EPUB.js (EPUB), PDF.js (PDF), calibre (MOBI)

### Implementation
See `nexus-core/src/validation.rs:validate_book()`

### Example
```bash
# Download from Project Gutenberg
wget https://www.gutenberg.org/ebooks/download/1661.epub.images
cp 1661.epub.images data/books/sherlock-holmes.epub

# Or place PDF directly
cp manual.pdf data/books/manuals/
```

---

## POIs: FlatGeoBuf, GeoJSON

### FlatGeoBuf (Recommended)
**Flat Geometries and Binary Format** — Efficient spatial data

- **Format Version**: FlatGeoBuf 3.0+
- **File Extension**: `.fgb`
- **Container**: Binary column-oriented format
- **Typical Size**: 10 MB - 1 GB
- **Advantages**: Fast access, compact, supports spatial indexing
- **Use Case**: Millions of points, lines, polygons

**File Structure**:
```
Header
Spatial Index
Feature Data (columns)
```

**Validation**:
- FGB magic number check
- Feature count verification
- Geometry validity (basic)

**Implementation**: See `nexus-core/src/validation.rs:validate_poi()`

### GeoJSON
**Geographic JSON Format** — Human-readable, web-friendly

- **Format Version**: RFC 7946
- **File Extensions**: `.geojson`, `.json`
- **Container**: JSON (UTF-8 text)
- **Typical Size**: 10 MB - 500 MB
- **Advantages**: Human-readable, widely supported
- **Disadvantages**: Larger file size than binary formats

**Structure**:
```json
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "geometry": {
        "type": "Point",
        "coordinates": [longitude, latitude]
      },
      "properties": {
        "name": "Location",
        "category": "waterhole"
      }
    }
  ]
}
```

**Validation**:
- JSON parse success
- FeatureCollection structure
- Coordinate validity (lon, lat range)

### Storage & Serving
- **Storage**: `data/poi/`
- **Subdirectories**: Supported (e.g., `data/poi/water_sources/`, `data/poi/medical/`)
- **Access**: `/api/content/poi` endpoint
- **Map Overlay**: Leaflet/OpenLayers with GeoJSON layer
- **Format**: Stored as-is (FGB or GeoJSON); served as GeoJSON to client

### Conversion
FlatGeoBuf ↔ GeoJSON tools:
- `flatgeobuf-rs` (Rust library)
- `fgb` CLI tool
- `ogr2ogr` (GDAL)

### Example
```bash
# Download OSM POIs (GeoJSON)
wget https://example.com/hospitals.geojson
cp hospitals.geojson data/poi/healthcare/

# Or FlatGeoBuf format
wget https://example.com/water_sources.fgb
cp water_sources.fgb data/poi/
```

---

## Content Discovery

### Automatic Detection
File extension determines content type:

| Extension | Type | Directory |
|-----------|------|-----------|
| .pmtiles | Map | maps/ |
| .epub | Book | books/ |
| .pdf | Book | books/ |
| .mobi | Book | books/ |
| .fgb | POI | poi/ |
| .geojson | POI | poi/ |
| .json | POI | poi/ |

### Unsupported Formats
- Unknown extensions → Warning logged, file skipped
- User can rename (e.g., `data.bin` → `data.pmtiles`)

### Subdirectories
All formats support arbitrary subdirectories:
```
data/
├── maps/
│   ├── regions/
│   │   ├── europe.pmtiles
│   │   └── asia.pmtiles
│   └── city-level/
│       └── london-detail.pmtiles
├── books/
│   ├── fiction/
│   │   └── novel.epub
│   └── reference/
│       └── manual.pdf
└── poi/
    ├── medical/
    │   └── hospitals.geojson
    └── water/
        ├── sources.fgb
        └── wells.geojson
```

All files in nested subdirectories are discovered automatically.

---

## Metadata Extraction

### For Each Content Type

**Maps (PMTiles)**:
- Title (from metadata)
- Zoom levels (min/max)
- Bounds (geographic extent)
- Tile format (vector, raster)

**Books**:
- Title
- Author
- Language
- Cover image (if EPUB)
- Page count (estimated)

**POIs**:
- Collection name
- Feature count
- Geometry types (Point, LineString, Polygon)
- Bounding box

### Storage
Metadata stored in-memory (fetched on-demand) or cached:
- **v0.1**: On-demand discovery
- **v0.2**: Metadata cache file (`.nexus-metadata.json`)

---

## Validation Rules (Warn-Only Strategy)

### Principles
1. **Trust User Files**: Accept all files, even if validation fails
2. **Warn, Don't Block**: Log warnings, ingest anyway
3. **Fail Gracefully**: Missing files → Empty list, invalid content → Show warning badge

### Validation Flow
```
File Uploaded
    ↓
Extract Extension
    ↓
Detect Content Type (Success/Unknown)
    ↓
Perform Format Validation
    ├─ Valid → "No warnings"
    ├─ Invalid Header → "Warning: file may not be [type]"
    └─ Read Error → "Warning: could not validate"
    ↓
Ingest File (Success/Failure)
```

### Rationale
- **User Autonomy**: Users may have valid files with atypical structures
- **Offline Resilience**: Don't reject content in off-grid scenario
- **Helpful Feedback**: Warnings guide user, don't prevent access

---

## Size Recommendations

| Type | Recommended | Maximum* |
|------|-------------|----------|
| Map (PMTiles) | 100 MB - 1 GB | 10 GB |
| Book (EPUB) | 10-50 MB | 500 MB |
| Book (PDF) | 10-100 MB | 1 GB |
| POI (FlatGeoBuf) | 50-500 MB | 2 GB |
| POI (GeoJSON) | 10-100 MB | 500 MB |

*Maximum based on typical storage constraints. Nexus scales to larger files; adjust hardware accordingly.

---

## Tools & Resources

### PMTiles
- `tippecanoe`: Create PMTiles from GIS data
- `pmtiles`: Validate and inspect PMTiles files
- @maplibre/pmtiles: Browser client

### Books
- Calibre: Manage and convert e-books
- `epub.js`: Browser EPUB reader
- `pdf.js`: Browser PDF viewer

### POIs
- `ogr2ogr`: GIS format conversion
- `fgb`: FlatGeoBuf CLI tools
- `geojson.io`: Online GeoJSON editor

---

## Future Format Support (v0.2+)

- **SQLite with SpatialIndex**: Efficient POI queries
- **MBTiles**: Alternative tile format
- **WEBP**: Raster tile compression
- **ComicBook**: CBZ/CBR formats for comics

---

**See Also**: [ARCHITECTURE.md](./ARCHITECTURE.md), [CONTENT_DOWNLOADER.md](./CONTENT_DOWNLOADER.md)
