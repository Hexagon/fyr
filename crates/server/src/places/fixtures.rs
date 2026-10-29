//! Test fixtures: real vector tiles inside real MBTiles and PMTiles files.
//!
//! Tiles are encoded with [`super::mvt::encode`]; MBTiles are written with
//! rusqlite (TMS rows, gzip tiles) and PMTiles v3 by a small writer with
//! gzip directories, optional leaf directories and run-length entries.

use super::mvt::encode::{self, TestFeature, TestValue};
use super::tiles::{lon_lat_to_tile, zoom_first_id, zxy_to_tile_id};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

/// A named feature placed by WGS84 coordinates.
#[derive(Debug, Clone)]
pub struct FixtureFeature {
    pub layer: &'static str,
    pub lon: f64,
    pub lat: f64,
    pub attrs: Vec<(&'static str, TestValue)>,
    /// Zoom levels whose tiles contain the feature.
    pub zooms: Vec<u8>,
}

pub fn feature(layer: &'static str, lon: f64, lat: f64, zooms: &[u8], attrs: &[(&'static str, TestValue)]) -> FixtureFeature {
    FixtureFeature {
        layer,
        lon,
        lat,
        attrs: attrs.to_vec(),
        zooms: zooms.to_vec(),
    }
}

pub const EXTENT: u32 = 4096;

/// Encode `features` into tiles: `(z, x, y) → MVT bytes` (uncompressed).
pub fn build_tiles(features: &[FixtureFeature]) -> BTreeMap<(u8, u32, u32), Vec<u8>> {
    let mut grouped: BTreeMap<(u8, u32, u32), BTreeMap<&'static str, Vec<TestFeature>>> = BTreeMap::new();
    for f in features {
        for &z in &f.zooms {
            let (x, y, fx, fy) = lon_lat_to_tile(z, f.lon, f.lat);
            let px = (fx * f64::from(EXTENT)).floor() as i32;
            let py = (fy * f64::from(EXTENT)).floor() as i32;
            let attrs: Vec<(&str, TestValue)> = f.attrs.iter().map(|(k, v)| (*k, v.clone())).collect();
            grouped
                .entry((z, x, y))
                .or_default()
                .entry(f.layer)
                .or_default()
                .push(TestFeature::point(px, py, &attrs));
        }
    }
    grouped
        .into_iter()
        .map(|(key, layers)| {
            let encoded: Vec<Vec<u8>> = layers
                .iter()
                .map(|(name, feats)| encode::layer(name, EXTENT, feats))
                .collect();
            (key, encode::tile(&encoded))
        })
        .collect()
}

/// Write an MBTiles file with gzip-compressed vector tiles.
pub fn write_mbtiles(path: &Path, tiles: &BTreeMap<(u8, u32, u32), Vec<u8>>, format: &str) {
    let _ = std::fs::remove_file(path);
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE metadata (name TEXT, value TEXT);
         CREATE TABLE tiles (zoom_level INTEGER, tile_column INTEGER, tile_row INTEGER, tile_data BLOB);
         CREATE UNIQUE INDEX tile_index ON tiles (zoom_level, tile_column, tile_row);",
    )
    .unwrap();
    let min = tiles.keys().map(|k| k.0).min().unwrap_or(0);
    let max = tiles.keys().map(|k| k.0).max().unwrap_or(0);
    for (k, v) in [("format", format.to_string()), ("minzoom", min.to_string()), ("maxzoom", max.to_string()), ("name", "test".to_string())] {
        conn.execute("INSERT INTO metadata VALUES (?1, ?2)", rusqlite::params![k, v]).unwrap();
    }
    for (&(z, x, y), data) in tiles {
        let row = (1u32 << z) - 1 - y;
        let blob = if format == "pbf" { gzip(data) } else { data.clone() };
        conn.execute(
            "INSERT INTO tiles VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![z, x, row, blob],
        )
        .unwrap();
    }
}

fn serialize_directory(entries: &[(u64, u64, u32, u32)]) -> Vec<u8> {
    // (tile_id, offset, length, run_length)
    let mut out = Vec::new();
    encode::varint(entries.len() as u64, &mut out);
    let mut last = 0u64;
    for e in entries {
        encode::varint(e.0 - last, &mut out);
        last = e.0;
    }
    for e in entries {
        encode::varint(u64::from(e.3), &mut out);
    }
    for e in entries {
        encode::varint(u64::from(e.2), &mut out);
    }
    for (i, e) in entries.iter().enumerate() {
        if i > 0 && e.1 == entries[i - 1].1 + u64::from(entries[i - 1].2) {
            encode::varint(0, &mut out);
        } else {
            encode::varint(e.1 + 1, &mut out);
        }
    }
    out
}

/// Options for [`write_pmtiles`].
#[derive(Debug, Clone, Copy)]
pub struct PmtilesOptions {
    /// Put entries into leaf directories of this many entries (0 = root only).
    pub leaf_size: usize,
    /// Merge consecutive tile IDs with identical bytes into runs.
    pub runs: bool,
    /// PMTiles tile type (1 = MVT, 2 = PNG).
    pub tile_type: u8,
}

impl Default for PmtilesOptions {
    fn default() -> Self {
        Self {
            leaf_size: 0,
            runs: true,
            tile_type: 1,
        }
    }
}

/// Write a PMTiles v3 file (gzip directories and tiles).
pub fn write_pmtiles(path: &Path, tiles: &BTreeMap<(u8, u32, u32), Vec<u8>>, options: PmtilesOptions) {
    let mut by_id: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    for (&(z, x, y), data) in tiles {
        let bytes = if options.tile_type == 1 { gzip(data) } else { data.clone() };
        by_id.insert(zxy_to_tile_id(z, x, y).unwrap(), bytes);
    }
    let mut data = Vec::new();
    let mut entries: Vec<(u64, u64, u32, u32)> = Vec::new();
    let mut last_bytes: Option<Vec<u8>> = None;
    for (id, bytes) in by_id {
        if options.runs {
            if let (Some(last), Some(prev)) = (entries.last_mut(), last_bytes.as_ref()) {
                if *prev == bytes && last.0 + u64::from(last.3) == id {
                    last.3 += 1;
                    continue;
                }
            }
        }
        entries.push((id, data.len() as u64, bytes.len() as u32, 1));
        data.extend_from_slice(&bytes);
        last_bytes = Some(bytes);
    }
    let (root, leaves) = if options.leaf_size == 0 {
        (gzip(&serialize_directory(&entries)), Vec::new())
    } else {
        let mut leaves = Vec::new();
        let mut root_entries = Vec::new();
        for chunk in entries.chunks(options.leaf_size) {
            let leaf = gzip(&serialize_directory(chunk));
            root_entries.push((chunk[0].0, leaves.len() as u64, leaf.len() as u32, 0));
            leaves.extend_from_slice(&leaf);
        }
        (gzip(&serialize_directory(&root_entries)), leaves)
    };
    let metadata = gzip(b"{}");
    let root_offset = 127u64;
    let metadata_offset = root_offset + root.len() as u64;
    let leaf_offset = metadata_offset + metadata.len() as u64;
    let data_offset = leaf_offset + leaves.len() as u64;
    let mut header = vec![0u8; 127];
    header[..7].copy_from_slice(b"PMTiles");
    header[7] = 3;
    let mut put = |at: usize, v: u64| header[at..at + 8].copy_from_slice(&v.to_le_bytes());
    put(8, root_offset);
    put(16, root.len() as u64);
    put(24, metadata_offset);
    put(32, metadata.len() as u64);
    put(40, leaf_offset);
    put(48, leaves.len() as u64);
    put(56, data_offset);
    put(64, data.len() as u64);
    put(72, tiles.len() as u64);
    put(80, entries.len() as u64);
    put(88, entries.len() as u64);
    header[96] = 0;
    header[97] = 2;
    header[98] = if options.tile_type == 1 { 2 } else { 1 };
    header[99] = options.tile_type;
    let min_zoom = tiles.keys().map(|k| k.0).min().unwrap_or(0);
    let max_zoom = tiles.keys().map(|k| k.0).max().unwrap_or(0);
    header[100] = min_zoom;
    header[101] = max_zoom;
    // Bounds and centre (E7 degrees) from the tiles of the lowest zoom.
    let (mut west, mut south, mut east, mut north) = (180.0f64, 90.0f64, -180.0f64, -90.0f64);
    for &(z, x, y) in tiles.keys().filter(|k| k.0 == min_zoom) {
        let (w, n) = super::tiles::tile_to_lon_lat(z, x, y, 0.0, 0.0, 1).unwrap();
        let (e, s) = super::tiles::tile_to_lon_lat(z, x, y, 1.0, 1.0, 1).unwrap();
        west = west.min(w);
        east = east.max(e);
        south = south.min(s);
        north = north.max(n);
    }
    let e7 = |v: f64| ((v * 1e7).round() as i32).to_le_bytes();
    header[102..106].copy_from_slice(&e7(west));
    header[106..110].copy_from_slice(&e7(south));
    header[110..114].copy_from_slice(&e7(east));
    header[114..118].copy_from_slice(&e7(north));
    header[118] = min_zoom;
    header[119..123].copy_from_slice(&e7((west + east) / 2.0));
    header[123..127].copy_from_slice(&e7((south + north) / 2.0));
    let mut file = Vec::new();
    file.extend_from_slice(&header);
    file.extend_from_slice(&root);
    file.extend_from_slice(&metadata);
    file.extend_from_slice(&leaves);
    file.extend_from_slice(&data);
    std::fs::write(path, file).unwrap();
}

/// The standard fixture: Swedish places in OpenMapTiles layers, plus a
/// few in the southern/western hemispheres.
pub fn omt_features() -> Vec<FixtureFeature> {
    use TestValue::Int;
    let all = [3u8, 6, 9, 12, 14];
    vec![
        feature("place", 18.0686, 59.3293, &all, &[("name", "Stockholm".into()), ("class", "city".into()), ("population", Int(975_000)), ("capital", Int(2)), ("rank", Int(1))]),
        feature("place", 11.9746, 57.7089, &all, &[("name", "Göteborg".into()), ("name:en", "Gothenburg".into()), ("class", "city".into()), ("population", Int(580_000))]),
        feature("place", 13.0038, 55.6050, &[6, 9, 12, 14], &[("name", "Malmö".into()), ("class", "city".into()), ("population", Int(350_000))]),
        feature("place", 13.0815, 63.3990, &[12, 14], &[("name", "Åre".into()), ("class", "village".into())]),
        feature("place", 14.0, 62.0, &[14], &[("name", "Berg".into()), ("class", "village".into())]),
        feature("place", 16.0, 58.0, &[14], &[("name", "Berg".into()), ("class", "village".into())]),
        feature("place", 15.5, 60.5, &[12, 14], &[("name", "Bergby".into()), ("class", "town".into()), ("population", Int(20_000))]),
        feature("place", 16.2, 58.6, &[14], &[("name", "Sankt Anna".into()), ("class", "village".into())]),
        feature("place", 30.3141, 59.9386, &all, &[("name", "Sankt Petersburg".into()), ("class", "city".into()), ("population", Int(5_380_000))]),
        feature("mountain_peak", 18.5333, 67.9000, &[12, 14], &[("name", "Kebnekaise".into()), ("class", "peak".into()), ("ele", Int(2097))]),
        feature("water_name", 13.5, 58.9, &[6, 9, 12, 14], &[("name", "Vänern".into()), ("class", "lake".into())]),
        feature("poi", 18.8, 68.35, &[14], &[("name", "Abisko Turiststation".into()), ("class", "lodging".into())]),
        feature("poi", 18.0700, 59.3300, &[14], &[("class", "cafe".into())]),
        feature("place", 151.2093, -33.8688, &all, &[("name", "Sydney".into()), ("class", "city".into()), ("population", Int(5_300_000))]),
        feature("place", -70.6693, -33.4489, &all, &[("name", "Santiago".into()), ("class", "city".into()), ("population", Int(6_200_000))]),
        feature("place", -21.9426, 64.1466, &all, &[("name", "Reykjavík".into()), ("class", "city".into())]),
    ]
}

/// The same region in the Protomaps basemap schema.
pub fn protomaps_features() -> Vec<FixtureFeature> {
    use TestValue::Int;
    vec![
        feature("places", 18.0686, 59.3293, &[4, 8, 12], &[("name", "Stockholm".into()), ("kind", "locality".into()), ("kind_detail", "city".into()), ("population_rank", Int(14))]),
        feature("places", 20.2253, 67.8558, &[8, 12], &[("name", "Kiruna".into()), ("name:sv", "Kiruna".into()), ("kind", "locality".into()), ("kind_detail", "town".into()), ("population", Int(17_000))]),
        feature("pois", 18.5333, 67.9000, &[12], &[("name", "Kebnekaise".into()), ("kind", "peak".into())]),
        feature("water", 14.6, 58.4, &[8, 12], &[("name", "Vättern".into()), ("kind", "lake".into())]),
        feature("places", 174.7633, -36.8485, &[4, 8, 12], &[("name", "Auckland".into()), ("kind", "locality".into()), ("kind_detail", "city".into())]),
    ]
}

/// Raw bytes of a tile that is not a vector tile.
pub fn garbage_tile() -> Vec<u8> {
    vec![0x1a, 0xff, 0xff, 0xff, 0xff, 0x0f, 0x00]
}

/// First tile ID of a zoom (re-exported for tests).
pub fn first_id(z: u8) -> u64 {
    zoom_first_id(z)
}
