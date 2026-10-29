//! Place search for vector maps.
//!
//! Maps are the one content type whose text (place names) global search
//! could not see: names live inside compressed vector tiles. A background
//! worker extracts named features from place-like layers of every vector map
//! (PMTiles or MBTiles) into one SQLite FTS5 database per map, and global
//! search (`kind = place`) and the Maps page query those databases.
//!
//! * [`mvt`]: minimal bounds-checked protobuf / vector-tile decoder.
//! * [`tiles`]: PMTiles v3 and MBTiles readers, decompression, tile math.
//! * [`store`]: the per-map index database (schema, writer with
//!   de-duplication, read-only queries).
//! * [`worker`]: the throttled background indexer and admin controls.
//! * [`routes`]: `GET /api/places/status` (public) and the protected
//!   `POST /api/places/:filename/build` / `DELETE /api/places/:filename`.
//!
//! Which layers and attributes count is data, not code: see
//! [`LAYER_RULES`], [`KIND_RULES`] and [`NAME_KEYS`]. Unknown layers are
//! ignored, unknown kinds fall back to their layer's category.
//!
//! Index files live in `DATA_DIR/index/` (never served) as
//! `places-<map stem>-<hash of filename>.sqlite`; only filenames, names and
//! coordinates leave this module.

pub mod mvt;
pub mod routes;
pub mod store;
pub mod tiles;
pub mod worker;

#[cfg(test)]
pub mod fixtures;
#[cfg(test)]
mod tests;

pub use store::PlaceReader;
pub use worker::PlaceIndexer;

use crate::fulltext::ArchiveStamp;
use std::path::{Path, PathBuf};

/// Longest place name stored (characters).
pub const MAX_NAME_CHARS: usize = 200;
/// Alternative names kept per place.
pub const MAX_ALT_NAMES: usize = 6;
/// Named features taken from one tile.
pub const MAX_CANDIDATES_PER_TILE: usize = 5_000;
/// Highest zoom read; deeper tiles repeat what z14 already holds.
pub const MAX_INDEX_ZOOM: u8 = 14;
/// Zoom passes (ascending, so important places become searchable first),
/// clipped to the map's range; the map's max zoom (≤ 14) is always added.
pub const ZOOM_PASSES: [u8; 4] = [3, 6, 9, 12];

/// Coarse class of a place; drives de-duplication radius, default
/// importance and the zoom of links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Place,
    Water,
    Natural,
    Park,
    Transport,
    Poi,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Place => "place",
            Category::Water => "water",
            Category::Natural => "natural",
            Category::Park => "park",
            Category::Transport => "transport",
            Category::Poi => "poi",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "place" => Category::Place,
            "water" => Category::Water,
            "natural" => Category::Natural,
            "park" => Category::Park,
            "transport" => Category::Transport,
            "poi" => Category::Poi,
            _ => return None,
        })
    }

    /// Same name and category within this distance count as one place
    /// (the same label repeated across zoom levels, or a line/area label
    /// repeated in neighbouring tiles).
    pub fn dedup_radius_km(self) -> f64 {
        match self {
            Category::Place => 3.0,
            Category::Water => 10.0,
            Category::Natural => 1.0,
            Category::Park => 3.0,
            Category::Transport => 3.0,
            Category::Poi => 0.3,
        }
    }

    /// Importance and link zoom for kinds not in [`KIND_RULES`].
    fn defaults(self) -> (f64, u8) {
        match self {
            Category::Place => (15.0, 14),
            Category::Water => (15.0, 12),
            Category::Natural => (15.0, 14),
            Category::Park => (12.0, 14),
            Category::Transport => (15.0, 14),
            Category::Poi => (5.0, 17),
        }
    }

    fn default_kind(self) -> &'static str {
        match self {
            Category::Place => "place",
            Category::Water => "water",
            Category::Natural => "peak",
            Category::Park => "park",
            Category::Transport => "aerodrome",
            Category::Poi => "poi",
        }
    }
}

/// A vector-tile layer that carries named places.
#[derive(Debug, Clone, Copy)]
pub struct LayerRule {
    pub layer: &'static str,
    pub category: Category,
    /// Only Point features (other geometries in the layer are unnamed
    /// fills, or would repeat the label in every tile they cross).
    pub points_only: bool,
    /// Attributes holding the kind, in order of preference.
    pub kind_keys: &'static [&'static str],
}

const PLACE_KIND_KEYS: &[&str] = &["kind_detail", "type", "class", "kind", "place"];
const WATER_KIND_KEYS: &[&str] = &["class", "kind_detail", "kind", "type"];
const NATURAL_KIND_KEYS: &[&str] = &["class", "kind", "type", "natural"];
const POI_KIND_KEYS: &[&str] = &[
    "kind", "subclass", "type", "class", "amenity", "shop", "tourism", "leisure", "natural", "historic", "aeroway",
    "railway",
];

/// Layer schemas understood: OpenMapTiles, Protomaps basemaps, Shortbread
/// and Mapbox Streets.
pub const LAYER_RULES: &[LayerRule] = &[
    // OpenMapTiles
    LayerRule { layer: "place", category: Category::Place, points_only: true, kind_keys: PLACE_KIND_KEYS },
    LayerRule { layer: "water_name", category: Category::Water, points_only: false, kind_keys: WATER_KIND_KEYS },
    LayerRule { layer: "mountain_peak", category: Category::Natural, points_only: true, kind_keys: NATURAL_KIND_KEYS },
    LayerRule { layer: "aerodrome_label", category: Category::Transport, points_only: true, kind_keys: &["class", "kind"] },
    LayerRule { layer: "park", category: Category::Park, points_only: true, kind_keys: &["class", "kind"] },
    LayerRule { layer: "poi", category: Category::Poi, points_only: true, kind_keys: POI_KIND_KEYS },
    // Protomaps basemaps
    LayerRule { layer: "places", category: Category::Place, points_only: true, kind_keys: PLACE_KIND_KEYS },
    LayerRule { layer: "pois", category: Category::Poi, points_only: true, kind_keys: POI_KIND_KEYS },
    LayerRule { layer: "water", category: Category::Water, points_only: true, kind_keys: WATER_KIND_KEYS },
    LayerRule { layer: "physical_point", category: Category::Natural, points_only: true, kind_keys: NATURAL_KIND_KEYS },
    // Shortbread
    LayerRule { layer: "place_labels", category: Category::Place, points_only: true, kind_keys: PLACE_KIND_KEYS },
    LayerRule { layer: "water_polygons_labels", category: Category::Water, points_only: true, kind_keys: WATER_KIND_KEYS },
    LayerRule { layer: "water_lines_labels", category: Category::Water, points_only: false, kind_keys: WATER_KIND_KEYS },
    // Mapbox Streets
    LayerRule { layer: "place_label", category: Category::Place, points_only: true, kind_keys: PLACE_KIND_KEYS },
    LayerRule { layer: "natural_label", category: Category::Natural, points_only: false, kind_keys: NATURAL_KIND_KEYS },
    LayerRule { layer: "water_label", category: Category::Water, points_only: false, kind_keys: WATER_KIND_KEYS },
    LayerRule { layer: "airport_label", category: Category::Transport, points_only: true, kind_keys: &["class", "type"] },
    LayerRule { layer: "poi_label", category: Category::Poi, points_only: true, kind_keys: POI_KIND_KEYS },
];

pub fn layer_rule(name: &str) -> Option<&'static LayerRule> {
    LAYER_RULES.iter().find(|rule| rule.layer == name)
}

/// Known kinds: `(kind, category, base importance 0–70, link zoom)`. A
/// kind listed here also moves a feature to its category (a `peak` in a
/// POI layer is a natural feature).
pub const KIND_RULES: &[(&str, Category, f64, u8)] = &[
    ("continent", Category::Place, 70.0, 3),
    ("country", Category::Place, 60.0, 5),
    ("state", Category::Place, 50.0, 7),
    ("province", Category::Place, 50.0, 7),
    ("region", Category::Place, 48.0, 7),
    ("archipelago", Category::Place, 30.0, 9),
    ("county", Category::Place, 40.0, 9),
    ("city", Category::Place, 45.0, 11),
    ("town", Category::Place, 35.0, 12),
    ("municipality", Category::Place, 30.0, 12),
    ("island", Category::Place, 25.0, 12),
    ("village", Category::Place, 25.0, 14),
    ("borough", Category::Place, 20.0, 13),
    ("suburb", Category::Place, 20.0, 14),
    ("quarter", Category::Place, 18.0, 15),
    ("neighbourhood", Category::Place, 15.0, 15),
    ("neighborhood", Category::Place, 15.0, 15),
    ("hamlet", Category::Place, 15.0, 15),
    ("locality", Category::Place, 15.0, 15),
    ("islet", Category::Place, 12.0, 15),
    ("isolated_dwelling", Category::Place, 10.0, 16),
    ("farm", Category::Place, 8.0, 16),
    ("ocean", Category::Water, 50.0, 4),
    ("sea", Category::Water, 45.0, 6),
    ("bay", Category::Water, 25.0, 11),
    ("strait", Category::Water, 25.0, 11),
    ("fjord", Category::Water, 25.0, 11),
    ("lake", Category::Water, 25.0, 12),
    ("lagoon", Category::Water, 20.0, 12),
    ("river", Category::Water, 20.0, 12),
    ("reservoir", Category::Water, 20.0, 13),
    ("canal", Category::Water, 12.0, 14),
    ("stream", Category::Water, 8.0, 15),
    ("pond", Category::Water, 8.0, 15),
    ("mountain_range", Category::Natural, 30.0, 10),
    ("glacier", Category::Natural, 25.0, 11),
    ("volcano", Category::Natural, 25.0, 13),
    ("peak", Category::Natural, 20.0, 14),
    ("ridge", Category::Natural, 15.0, 13),
    ("cape", Category::Natural, 15.0, 13),
    ("hill", Category::Natural, 12.0, 15),
    ("saddle", Category::Natural, 10.0, 15),
    ("national_park", Category::Park, 30.0, 10),
    ("nature_reserve", Category::Park, 22.0, 12),
    ("protected_area", Category::Park, 22.0, 12),
    ("park", Category::Park, 12.0, 14),
    ("airport", Category::Transport, 25.0, 13),
    ("aerodrome", Category::Transport, 25.0, 13),
];

fn kind_rule(kind: &str) -> Option<&'static (&'static str, Category, f64, u8)> {
    KIND_RULES.iter().find(|(k, ..)| *k == kind)
}

/// Zoom a link to a place of this kind/category should open at.
pub fn zoom_for(kind: &str, category: Category) -> u8 {
    kind_rule(kind).map_or_else(|| category.defaults().1, |rule| rule.3)
}

/// Attributes read as the primary name, in order of preference.
pub const NAME_KEYS: &[&str] = &["name", "name:latin", "name_int", "name_en", "name:en"];
/// Attributes stored as alternative (searchable) names when they differ.
pub const ALT_NAME_KEYS: &[&str] = &["name:sv", "name:en", "name_en", "name_sv", "name_int", "name:latin"];

/// A named feature found in one tile, in tile-local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub name: String,
    pub alt: Vec<String>,
    pub category: Category,
    pub kind: String,
    pub px: f64,
    pub py: f64,
    pub extent: u32,
    /// Importance from attributes (kind, population, rank, capital), 0–100,
    /// before the zoom bonus.
    pub base_importance: f64,
}

/// A place ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub alt: Vec<String>,
    pub category: Category,
    pub kind: String,
    pub lon: f64,
    pub lat: f64,
    pub importance: f64,
    /// Zoom of the tile it was found in.
    pub zoom: u8,
}

/// Trim, collapse whitespace, drop control and private-use characters and
/// cap the length; `None` when nothing is left.
pub fn clean_name(raw: &str) -> Option<String> {
    let mut out = String::new();
    for word in raw.split_whitespace() {
        let word: String = word
            .chars()
            .filter(|c| !c.is_control() && !('\u{E000}'..='\u{F8FF}').contains(c))
            .collect();
        if word.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&word);
    }
    let out: String = out.chars().take(MAX_NAME_CHARS).collect();
    let out = out.trim_end().to_string();
    (!out.is_empty()).then_some(out)
}

/// Normalise a kind value: lowercase `[a-z0-9_]`, spaces/dashes to `_`,
/// at most 40 characters.
pub fn clean_kind(raw: &str) -> Option<String> {
    let kind: String = raw
        .trim()
        .chars()
        .map(|c| match c {
            ' ' | '-' | ':' => '_',
            c => c.to_ascii_lowercase(),
        })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(40)
        .collect();
    (!kind.is_empty()).then_some(kind)
}

/// Importance bonus for a place first seen at `zoom`: labels present at low
/// zooms are the ones the map maker considered important.
pub fn zoom_bonus(zoom: u8) -> f64 {
    f64::from(MAX_INDEX_ZOOM.saturating_sub(zoom))
}

fn attribute_importance(layer: &mvt::Layer<'_>, feature: &mvt::Feature, category: Category) -> f64 {
    let number = |key: &str| layer.attr(feature, key).and_then(|v| v.as_f64());
    let mut bonus = 0.0;
    if let Some(population) = number("population").or_else(|| number("pop")).filter(|p| *p > 0.0) {
        bonus += (3.0 * (population + 1.0).log10()).min(20.0);
    }
    if category == Category::Place {
        // OpenMapTiles: rank 1 is the most important.
        if let Some(rank) = number("rank").filter(|r| *r >= 1.0) {
            bonus += ((10.0 - rank).clamp(0.0, 10.0)) * 0.5;
        }
        // Protomaps: higher population_rank is more populous.
        if let Some(rank) = number("population_rank") {
            bonus += rank.clamp(0.0, 18.0) * 0.4;
        }
        let capital = layer.attr(feature, "capital");
        match capital.and_then(|v| v.as_f64()).map(|c| c as i64) {
            Some(2) => bonus += 8.0,
            Some(3 | 4) => bonus += 4.0,
            _ => {
                if capital.and_then(|v| v.as_str()).is_some_and(|s| s.eq_ignore_ascii_case("yes")) {
                    bonus += 8.0;
                }
            }
        }
    }
    bonus
}

/// Named features of the place-like layers of one decoded tile.
pub fn extract_candidates(tile: &[u8]) -> Result<Vec<Candidate>, mvt::DecodeError> {
    let layers = mvt::decode_layers(tile, |name| layer_rule(name).is_some())?;
    let mut out = Vec::new();
    for layer in &layers {
        let Some(rule) = layer_rule(layer.name) else { continue };
        let extent = f64::from(layer.extent);
        for index in 0..layer.feature_count() {
            if out.len() >= MAX_CANDIDATES_PER_TILE {
                return Ok(out);
            }
            let Ok(feature) = layer.feature(index) else { continue };
            let geom_type = feature.geom_type.unwrap_or(mvt::GeomType::Unknown);
            if rule.points_only && geom_type != mvt::GeomType::Point {
                continue;
            }
            let Some(name) = NAME_KEYS
                .iter()
                .find_map(|key| layer.attr(&feature, key).and_then(|v| v.as_str()).and_then(clean_name))
            else {
                continue;
            };
            let Ok(Some((px, py))) = mvt::representative_point(geom_type, &feature.geometry) else {
                continue;
            };
            // Features in the tile buffer are repeated by the neighbour that
            // owns them.
            if !(0.0..extent).contains(&px) || !(0.0..extent).contains(&py) {
                continue;
            }
            let raw_kind = rule
                .kind_keys
                .iter()
                .find_map(|key| layer.attr(&feature, key).and_then(|v| v.as_str()).and_then(clean_kind));
            let (category, kind) = match raw_kind.as_deref().and_then(kind_rule) {
                Some((kind, category, ..)) => (*category, kind.to_string()),
                None => (
                    rule.category,
                    raw_kind.unwrap_or_else(|| rule.category.default_kind().to_string()),
                ),
            };
            let folded = crate::search::fold(&name);
            let mut alt: Vec<String> = Vec::new();
            let mut seen = vec![folded];
            for key in ALT_NAME_KEYS {
                if alt.len() >= MAX_ALT_NAMES {
                    break;
                }
                if let Some(value) = layer.attr(&feature, key).and_then(|v| v.as_str()).and_then(clean_name) {
                    let f = crate::search::fold(&value);
                    if !seen.contains(&f) {
                        seen.push(f);
                        alt.push(value);
                    }
                }
            }
            let base = kind_rule(&kind).map_or_else(|| category.defaults().0, |rule| rule.2);
            let base_importance = (base + attribute_importance(layer, &feature, category)).clamp(0.0, 100.0);
            out.push(Candidate {
                name,
                alt,
                category,
                kind,
                px,
                py,
                extent: layer.extent,
                base_importance,
            });
        }
    }
    Ok(out)
}

/// Place a tile-local candidate on the globe.
pub fn locate(candidate: &Candidate, z: u8, x: u32, y: u32) -> Option<Place> {
    let (lon, lat) = tiles::tile_to_lon_lat(z, x, y, candidate.px, candidate.py, candidate.extent)?;
    Some(Place {
        name: candidate.name.clone(),
        alt: candidate.alt.clone(),
        category: candidate.category,
        kind: candidate.kind.clone(),
        lon,
        lat,
        importance: (candidate.base_importance + zoom_bonus(z)).min(100.0),
        zoom: z,
    })
}

/// Zoom levels read for a map with tiles in `min..=max`, ascending.
pub fn zoom_plan(min_zoom: u8, max_zoom: u8) -> Vec<u8> {
    let top = max_zoom.min(MAX_INDEX_ZOOM);
    if min_zoom > top {
        return Vec::new();
    }
    let mut zooms: Vec<u8> = ZOOM_PASSES.iter().copied().filter(|z| (min_zoom..top).contains(z)).collect();
    zooms.push(top);
    zooms
}

/// Great-circle distance in kilometres.
pub fn haversine_km(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0088 * a.sqrt().min(1.0).asin()
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// `places-<sanitised stem>-<12 hex chars of SHA-256(filename)>.sqlite`.
pub fn index_file_name(map: &str) -> String {
    let stem = Path::new(map).file_stem().and_then(|s| s.to_str()).unwrap_or(map);
    let safe: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(60)
        .collect();
    let digest = ring::digest::digest(&ring::digest::SHA256, map.as_bytes());
    format!("places-{safe}-{}.sqlite", hex::encode(&digest.as_ref()[..6]))
}

pub fn index_path(index_dir: &Path, map: &str) -> PathBuf {
    index_dir.join(index_file_name(map))
}

/// Whether `name` looks like one of our index files (for orphan cleanup).
pub fn is_index_file_name(name: &str) -> bool {
    name.starts_with("places-") && name.ends_with(".sqlite")
}

/// Map archive format by extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapFormat {
    Pmtiles,
    Mbtiles,
}

impl MapFormat {
    pub fn of(name: &str) -> Option<Self> {
        let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "pmtiles" => Some(MapFormat::Pmtiles),
            "mbtiles" => Some(MapFormat::Mbtiles),
            _ => None,
        }
    }
}

/// Whether `name` is an acceptable bare map filename for the places API.
pub fn is_valid_map_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && MapFormat::of(name).is_some()
}

/// What a map holds (vector zoom range, raster, unreadable). Blocking.
pub fn probe_map(path: &Path, format: MapFormat) -> tiles::MapContent {
    let probe = std::panic::catch_unwind(|| match format {
        MapFormat::Pmtiles => tiles::PmtilesArchive::open(path).map(|a| a.content()),
        MapFormat::Mbtiles => tiles::MbtilesArchive::open(path).map(|a| a.content()),
    });
    match probe {
        Ok(Ok(content)) => content,
        Ok(Err(reason)) => tiles::MapContent::Unsupported(reason),
        Err(_) => tiles::MapContent::Unsupported("The map could not be read".to_string()),
    }
}

/// Open the place index of `map` (stored at `map_path`) for queries, if it
/// exists, belongs to the map's current version and holds any places
/// (ready, or still being built).
pub fn open_reader(index_dir: &Path, map: &str, map_path: &Path) -> Option<PlaceReader> {
    let stamp = ArchiveStamp::of(map_path)?;
    let path = index_path(index_dir, map);
    if !path.is_file() {
        return None;
    }
    let reader = PlaceReader::open(&path).ok()?;
    let usable = reader.meta.matches(map, &stamp)
        && matches!(
            reader.meta.state,
            crate::fulltext::IndexState::Ready | crate::fulltext::IndexState::Building
        )
        && reader.meta.places > 0;
    usable.then_some(reader)
}
