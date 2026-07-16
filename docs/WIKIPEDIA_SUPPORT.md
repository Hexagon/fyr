# Wikipedia Support & Offline Content Presets

Offline Nexus can serve as a platform for **offline Wikipedia snapshots** and other curated content bundles. This guide explains how to download, configure, and deploy offline Wikipedia content.

## Overview

Rather than hosting Wikipedia snapshots directly in Offline Nexus, we leverage existing, battle-tested platforms:

- **Kiwix** (https://kiwix.org) — Production-ready offline content library
- **Wikipedia-to-Static** generators — Custom extraction and caching
- **Internet Archive** — Historical Wikipedia snapshots

---

## Option 1: Using Kiwix Snapshots (Recommended for v0.1)

Kiwix maintains pre-built, compressed Wikipedia snapshots in multiple languages and content levels. These are ideal for Offline Nexus deployment.

### Step 1: Download Kiwix Content

**Available content**:
- **Full Wikipedia** — Complete English Wikipedia with images (~90 GB, uncompressed)
- **Wikipedia with Images** — Full with media (~45 GB compressed)
- **Wikipedia without Images** — Text-only (~12 GB compressed)
- **Simplified Wikipedia** — Basic articles only (~2 GB)
- **Other languages** — Spanish, French, German, Chinese, Arabic, etc.

**Download location**: https://library.kiwix.org/

**Command line**:
```bash
# Download specific snapshot (example: English Wikipedia no images, ~12 GB)
wget https://library.kiwix.org/download/wikipedia_en_no_images.zim

# Or use aria2 for faster multi-threaded download
aria2c -x 4 -s 4 https://library.kiwix.org/download/wikipedia_en_no_images.zim
```

**Note**: File extensions are typically `.zim` (Zeno Interchange Markup). These are NOT PMTiles.

---

## Option 2: Converting Kiwix/ZIM to Searchable Format

To make Wikipedia searchable and integrated with Offline Nexus, convert ZIM snapshots to searchable formats:

### ZIM to Static HTML + Full-Text Index

**Tool**: `zim-tools` (Rust-based, lightweight)

```bash
# Installation
cargo install zim-tools

# Export ZIM to static HTML
zim-tools dump-zim wikipedia_en_no_images.zim --output-dir ./wikipedia/

# Creates: ./wikipedia/index.html + article files
```

### Indexing for Full-Text Search

Once exported, index with Lunr.js or Tantivy:

```rust
// Pseudocode: In nexus-server, add full-text search
use tantivy::{Schema, Index};

let mut schema = Schema::builder()
    .add_text_field("title", TEXT | STORED)
    .add_text_field("body", TEXT)
    .build();

let index = Index::create_in_ram(schema);
// Index all Wikipedia articles
```

---

## Option 3: Pre-Generated Wikipedia-as-PMTiles (Future)

Future versions of Offline Nexus may include a **searchable Wikipedia as PMTiles** for integration with the map viewer.

**Status**: Under research. Requires:
1. Extracting Wikipedia article metadata to GeoJSON (articles with geocoordinates)
2. Converting to PMTiles for fast geographic browsing
3. Linking map tiles to full article text

**Roadmap**: v0.3+

---

## Offline Content Presets

Below are **recommended offline bundles** for different use cases. Each includes Wikipedia subsets tailored to the scenario.

### Preset 1: Survival Kit (2 GB)

**Use case**: Emergency response, disaster zones, remote expeditions

**Contents**:
- Simplified Wikipedia (text-only, ~500 MB)
- **Medical articles**: Drugs, diseases, first aid, anatomy
- **Agriculture**: Crops, livestock, composting, water treatment
- **Engineering**: Basic mechanical systems, electrical safety
- **Navigation**: Maps, orienteering, GPS
- **Survival skills**: Shelter, fire, water, food

**Download command**:
```bash
# Create preset directory
mkdir -p data/books/survival-kit
cd data/books/survival-kit

# Wikipedia simplified
wget https://library.kiwix.org/download/wikipedia_en_simple.zim

# Optional: Add specialized guides
wget https://library.kiwix.org/download/wikibooks_en.zim  # Practical guides
```

### Preset 2: Medical & Health (3 GB)

**Use case**: Clinics, remote hospitals, health workers

**Contents**:
- Full medical Wikipedia
- DrugBank offline snapshot
- OpenMRS medical dictionary
- Emergency medicine protocols

**Integration**: Extracted to `/data/books/medical/` for full-text search

### Preset 3: Engineering & Mechanics (4 GB)

**Use case**: Mechanical repair, industrial maintenance, technical reference

**Contents**:
- Engineering Wikipedia articles
- Wikibooks technical manuals
- Open-source hardware documentation
- DIY repair guides

### Preset 4: Educational Core (1.5 GB)

**Use case**: Schools in areas with no internet, homeschooling

**Contents**:
- Full Wikipedia (text-only)
- Wikibooks (textbooks)
- Wikiversity (course materials)
- Simple diagrams (cached)

---

## Integration with Offline Nexus

### Method A: Import as Books

Place ZIM files or extracted content in `/data/books/`:

```bash
cp wikipedia_en_no_images.zim data/books/wikipedia.zim

# Or extract first
zim-tools dump-zim wikipedia_en_no_images.zim --output-dir data/books/wikipedia/
```

**Result**: Content appears in Nexus UI under "Books"

### Method B: Web-Based Reader

Offline Nexus will eventually support **web-based Wikipedia readers** (IPFS + static HTML viewer).

**Implementation** (roadmap):
1. Extract Wikipedia to static HTML in `/data/books/wikipedia/`
2. Serve via Axum static file handler
3. Add link in Nexus UI: "Browse Wikipedia"
4. Implement full-text search with Lunr.js

### Method C: API-Driven Search

Future Offline Nexus API endpoints:

```http
GET /api/search?q=antibiotics&category=medical
```

Returns matching Wikipedia articles with links to full text.

---

## Deployment Examples

### Scenario 1: Hospital in Rural Area

```bash
# 1. Create medical preset
mkdir offline-nexus-hospital
cd offline-nexus-hospital

# 2. Download medical Wikipedia
wget https://library.kiwix.org/download/wikipedia_en_medical_subset.zim
# Place in data/books/

# 3. Configure Nexus
./nexus --data-dir ./data/

# 4. Access on local network
# Doctor opens browser to http://hospital-server:8080
```

### Scenario 2: Humanitarian Response (Disaster)

```bash
# 1. Lightweight bundle (emergency response kit)
mkdir offline-nexus-emergency
cd offline-nexus-emergency

# 2. Download minimal Wikipedia + survival guides
wget https://library.kiwix.org/download/wikipedia_en_simple.zim
wget https://library.kiwix.org/download/wikibooks_en.zim

# 3. Pre-built Docker image
docker run -p 8080:8080 -v $(pwd)/data:/data offline-nexus-emergency:latest

# 4. Accessible at: http://192.168.1.X:8080
```

### Scenario 3: Field Research (Expedition)

```bash
# 1. Curated Wikipedia + maps
offline-nexus-research/
├── data/
│   ├── books/
│   │   ├── wikipedia_no_images.zim
│   │   └── geology_wikibooks.zim
│   ├── maps/
│   │   └── region_offline.pmtiles  # See PMTILES_GENERATION.md
│   └── poi/
│       └── research_stations.geojson

# 2. Deploy on portable device
# (Raspberry Pi + battery + local WiFi)
./nexus --port 8080
```

---

## Creating Custom Wikipedia Subsets

### Extract Specific Categories

Use **Pywikibot** or **MediaWiki XML dumps** to extract specific Wikipedia categories:

```bash
# Download Category:Medicine from Wikipedia
pywikibot -l:en -family:wikipedia -c

# Filter articles by category, save to new ZIM
# (Advanced users; Kiwix presets recommended for most)
```

### Add Local Content

Extend Wikipedia with local knowledge:

```bash
# Create new articles in static HTML
data/books/custom/
├── index.html
├── local-plants.html
├── community-health.html
└── emergency-contacts.html

# Serve via Nexus static file handler
```

---

## Roadmap: Wikipedia Integration (v0.2+)

### v0.2 (Q3 2026)
- ✅ Support ZIM files in book reader
- ✅ Full-text search with Lunr.js
- Link Wikipedia articles in offline maps

### v0.3 (Q4 2026)
- Geographic Wikipedia (articles on map by location)
- Wikipedia-as-PMTiles search
- Sync multiple Wikipedia versions

### v1.0 (2027)
- Peer-to-peer Wikipedia distribution (no central server)
- Real-time community annotations
- Multi-language full-text search

---

## Technical Notes

### ZIM File Format
- **Definition**: Zeno Interchange Markup — compressed offline content format
- **Size**: Highly compressed (~12-90 GB Wikipedia → 2-45 GB ZIM)
- **Reader tools**: Kiwix, ZIM tools, custom parsers
- **Advantages**: Single file, fast random access, good compression

### Performance Considerations

**File size impact**:
- Wikipedia text-only: +12 GB
- Wikipedia + images: +45 GB
- Medical subset: +3 GB
- Total Nexus deployment: 15-50 GB depending on content

**Deployment strategy**:
- **Small device** (Raspberry Pi): Use simplified/text-only Wikipedia (~2 GB)
- **Server** (Linux VM): Use full Wikipedia (+images, ~45 GB)
- **Mobile** (USB stick): Use survival kit preset (~2 GB)

### Search Performance

Full-text search on Wikipedia snapshots requires indexing:

- **Indexing time**: ~2-5 minutes (laptop SSD)
- **Search time**: <100ms per query (after indexing)
- **Index size**: ~10-30% of content size (Lunr.js default)

---

## FAQ

**Q: Can I sync Wikipedia updates?**
A: Offline Nexus v1.0 will include P2P sync. For now, download latest snapshots from Kiwix.

**Q: How do I search Wikipedia articles?**
A: Roadmap for v0.2. Currently, browse `/data/books/` manually or use browser Find.

**Q: Which Wikipedia language should I use?**
A: Kiwix supports 100+ languages. Choose based on community needs. English (45 GB) or Simple English (2 GB) recommended for first deployment.

**Q: Can I add my own content?**
A: Yes! Create articles as static HTML in `/data/books/custom/` or add to Wikipedia via Pywikibot.

**Q: What about copyright/attribution?**
A: Wikipedia content is CC-BY-SA licensed. Redistribute with attribution and include license text in bundle. See https://en.wikipedia.org/wiki/Wikipedia:Reusing_Wikipedia_content

---

## See Also

- [PMTILES_GENERATION.md](./PMTILES_GENERATION.md) — Offline maps with geographic data
- [DEVELOPER_DEPLOYMENT.md](./DEVELOPER_DEPLOYMENT.md) — Deploy Nexus on various hardware
- [API_REFERENCE.md](./API_REFERENCE.md) — Search and content endpoints
- Kiwix official: https://kiwix.org
- Wikipedia download: https://dump.wikipedia.org (raw data)
