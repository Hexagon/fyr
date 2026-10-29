//! Reading vector tiles out of map archives for the place indexer.
//!
//! * [`PmtilesArchive`]: PMTiles v3 — fixed 127-byte header, varint
//!   directories (optionally gzip/zstd compressed), leaf directories,
//!   Hilbert-curve tile IDs.
//! * [`MbtilesArchive`]: MBTiles — SQLite `tiles` table (TMS rows).
//! * Tile decompression (gzip, zlib, zstd) with an output cap, and the
//!   tile-local → WGS84 coordinate transform.
//!
//! Everything returns errors for malformed input; nothing here panics on
//! archive contents, and every allocation is bounded by a cap.

use rusqlite::{params, Connection, OpenFlags};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Highest zoom the tile ID math accepts (2^z must fit comfortably).
pub const MAX_ZOOM: u8 = 26;
/// Compressed tiles larger than this are skipped.
pub const MAX_TILE_BYTES: usize = 8 * 1024 * 1024;
/// Decompressed tiles larger than this are skipped.
pub const MAX_DECODED_TILE_BYTES: usize = 32 * 1024 * 1024;
/// Largest (decompressed) PMTiles directory accepted.
const MAX_DIRECTORY_BYTES: usize = 64 * 1024 * 1024;
/// Deepest leaf-directory nesting followed (the spec needs at most 3).
const MAX_DIRECTORY_DEPTH: usize = 6;

/// What kind of tiles an archive holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapContent {
    /// Mapbox vector tiles with the given zoom range.
    Vector { min_zoom: u8, max_zoom: u8 },
    /// Raster images: there are no names to index.
    Raster,
    /// Something the indexer cannot read (short, path-free reason).
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Zstd,
    /// Brotli or unknown: not supported.
    Other(u8),
}

impl Compression {
    fn from_pmtiles(value: u8) -> Self {
        match value {
            0 | 1 => Compression::None,
            2 => Compression::Gzip,
            4 => Compression::Zstd,
            other => Compression::Other(other),
        }
    }
}

// ---------------------------------------------------------------------------
// Decompression and coordinates (pure)
// ---------------------------------------------------------------------------

fn read_capped(mut reader: impl Read, cap: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    // Read one byte past the cap to detect overflow.
    reader.by_ref().take(cap as u64 + 1).read_to_end(&mut out).ok()?;
    (out.len() <= cap).then_some(out)
}

/// Decompress `data` (declared `compression`, or sniffed when `None`),
/// returning at most `cap` bytes; `None` when it is corrupt, too large or
/// uses an unsupported codec.
pub fn decompress(data: &[u8], compression: Option<Compression>, cap: usize) -> Option<Vec<u8>> {
    let compression = compression.unwrap_or_else(|| sniff_compression(data));
    match compression {
        Compression::None => (data.len() <= cap).then(|| data.to_vec()),
        Compression::Gzip => {
            if data.starts_with(&[0x1f, 0x8b]) {
                read_capped(flate2::read::MultiGzDecoder::new(data), cap)
            } else {
                read_capped(flate2::read::ZlibDecoder::new(data), cap)
            }
        }
        Compression::Zstd => read_capped(zstd::stream::read::Decoder::new(data).ok()?, cap),
        Compression::Other(_) => None,
    }
}

/// Guess a tile's compression from its first bytes (MBTiles has no header
/// for it): gzip, zlib (deflate with a zlib header) or zstd, else raw.
pub fn sniff_compression(data: &[u8]) -> Compression {
    match data {
        [0x1f, 0x8b, ..] => Compression::Gzip,
        [0x28, 0xb5, 0x2f, 0xfd, ..] => Compression::Zstd,
        [first, second, ..] if first & 0x0f == 8 && (u16::from(*first) << 8 | u16::from(*second)) % 31 == 0 => {
            Compression::Gzip
        }
        _ => Compression::None,
    }
}

/// Whether bytes look like a raster image (PNG, JPEG, WebP, GIF, AVIF).
pub fn looks_like_image(data: &[u8]) -> bool {
    data.starts_with(&[0x89, b'P', b'N', b'G'])
        || data.starts_with(&[0xff, 0xd8, 0xff])
        || (data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP")
        || data.starts_with(b"GIF8")
        || (data.len() >= 12 && &data[4..8] == b"ftyp")
}

/// Web-Mercator latitude limit.
pub const MAX_LATITUDE: f64 = 85.051_128_779_806_59;

/// WGS84 `(lon, lat)` of tile-local position `(px, py)` (0..extent, y down)
/// in tile `z/x/y` (XYZ scheme). `None` for out-of-range tiles, a zero
/// extent or non-finite input. Positions in the tile buffer (slightly
/// outside 0..extent) are allowed; the result is clamped to the valid range.
pub fn tile_to_lon_lat(z: u8, x: u32, y: u32, px: f64, py: f64, extent: u32) -> Option<(f64, f64)> {
    if z > MAX_ZOOM || extent == 0 || !px.is_finite() || !py.is_finite() {
        return None;
    }
    let n = f64::from(1u32 << z);
    if f64::from(x) >= n || f64::from(y) >= n {
        return None;
    }
    let extent = f64::from(extent);
    let wx = (f64::from(x) + px / extent) / n;
    let wy = (f64::from(y) + py / extent) / n;
    let lon = (wx * 360.0 - 180.0).clamp(-180.0, 180.0);
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * wy)).sinh().atan().to_degrees();
    let lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE);
    (lon.is_finite() && lat.is_finite()).then_some((lon, lat))
}

/// The XYZ tile containing `(lon, lat)` at zoom `z`, plus the fractional
/// position inside it (test and fixture helper).
#[cfg(test)]
pub fn lon_lat_to_tile(z: u8, lon: f64, lat: f64) -> (u32, u32, f64, f64) {
    let n = f64::from(1u32 << z);
    let wx = (lon + 180.0) / 360.0 * n;
    let lat_rad = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
    let wy = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    let x = (wx.floor() as u32).min((1u32 << z) - 1);
    let y = (wy.floor() as u32).min((1u32 << z) - 1);
    (x, y, wx - f64::from(x), wy - f64::from(y))
}

// ---------------------------------------------------------------------------
// PMTiles tile IDs (Hilbert curve)
// ---------------------------------------------------------------------------

/// First tile ID of zoom `z`: (4^z - 1) / 3.
pub fn zoom_first_id(z: u8) -> u64 {
    ((1u64 << (2 * u32::from(z))) - 1) / 3
}

#[cfg(test)]
pub fn zxy_to_tile_id(z: u8, x: u32, y: u32) -> Option<u64> {
    if z > MAX_ZOOM {
        return None;
    }
    let n = 1u64 << z;
    let (mut tx, mut ty) = (u64::from(x), u64::from(y));
    if tx >= n || ty >= n {
        return None;
    }
    let mut d = 0u64;
    let mut s = n / 2;
    while s > 0 {
        let rx = u64::from(tx & s > 0);
        let ry = u64::from(ty & s > 0);
        d += s * s * ((3 * rx) ^ ry);
        rotate(n, &mut tx, &mut ty, rx, ry);
        s /= 2;
    }
    Some(zoom_first_id(z) + d)
}

fn rotate(n: u64, x: &mut u64, y: &mut u64, rx: u64, ry: u64) {
    if ry == 0 {
        if rx == 1 {
            *x = n - 1 - *x;
            *y = n - 1 - *y;
        }
        std::mem::swap(x, y);
    }
}

pub fn tile_id_to_zxy(id: u64) -> Option<(u8, u32, u32)> {
    let mut z = 0u8;
    while z <= MAX_ZOOM {
        let next = zoom_first_id(z + 1);
        if id < next {
            let n = 1u64 << z;
            let mut t = id - zoom_first_id(z);
            let (mut x, mut y) = (0u64, 0u64);
            let mut s = 1u64;
            while s < n {
                let rx = 1 & (t / 2);
                let ry = 1 & (t ^ rx);
                rotate(s, &mut x, &mut y, rx, ry);
                x += s * rx;
                y += s * ry;
                t /= 4;
                s *= 2;
            }
            return Some((z, x as u32, y as u32));
        }
        z += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// PMTiles archive
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct PmtilesHeader {
    pub root_offset: u64,
    pub root_length: u64,
    pub leaf_offset: u64,
    pub leaf_length: u64,
    pub data_offset: u64,
    pub data_length: u64,
    pub internal_compression: Compression,
    pub tile_compression: Compression,
    pub tile_type: u8,
    pub min_zoom: u8,
    pub max_zoom: u8,
}

pub const PMTILES_HEADER_LEN: usize = 127;

pub fn parse_pmtiles_header(bytes: &[u8]) -> Result<PmtilesHeader, String> {
    if bytes.len() < PMTILES_HEADER_LEN || &bytes[0..7] != b"PMTiles" {
        return Err("Not a PMTiles archive".to_string());
    }
    if bytes[7] != 3 {
        return Err(format!("PMTiles version {} is not supported", bytes[7]));
    }
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap_or([0; 8]));
    Ok(PmtilesHeader {
        root_offset: u64_at(8),
        root_length: u64_at(16),
        leaf_offset: u64_at(40),
        leaf_length: u64_at(48),
        data_offset: u64_at(56),
        data_length: u64_at(64),
        internal_compression: Compression::from_pmtiles(bytes[97]),
        tile_compression: Compression::from_pmtiles(bytes[98]),
        tile_type: bytes[99],
        min_zoom: bytes[100],
        max_zoom: bytes[101],
    })
}

/// One directory entry. `run_length == 0` points at a leaf directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub tile_id: u64,
    pub offset: u64,
    pub length: u32,
    pub run_length: u32,
}

/// Parse a decompressed PMTiles directory.
pub fn parse_directory(data: &[u8]) -> Result<Vec<Entry>, String> {
    let mut reader = super::mvt::Reader::new(data);
    let bad = |_| "Malformed PMTiles directory".to_string();
    let count = reader.varint().map_err(bad)?;
    // Each entry needs at least four bytes: reject absurd counts before
    // allocating.
    if count > (data.len() as u64) / 4 + 1 {
        return Err("Malformed PMTiles directory".to_string());
    }
    let count = count as usize;
    let mut entries = vec![
        Entry {
            tile_id: 0,
            offset: 0,
            length: 0,
            run_length: 0
        };
        count
    ];
    let mut last_id = 0u64;
    for entry in entries.iter_mut() {
        let delta = reader.varint().map_err(bad)?;
        last_id = last_id.checked_add(delta).ok_or("Malformed PMTiles directory")?;
        entry.tile_id = last_id;
    }
    for entry in entries.iter_mut() {
        entry.run_length = u32::try_from(reader.varint().map_err(bad)?).map_err(|_| "Malformed PMTiles directory")?;
    }
    for entry in entries.iter_mut() {
        entry.length = u32::try_from(reader.varint().map_err(bad)?).map_err(|_| "Malformed PMTiles directory")?;
    }
    for i in 0..count {
        let raw = reader.varint().map_err(bad)?;
        entries[i].offset = if raw == 0 && i > 0 {
            entries[i - 1]
                .offset
                .checked_add(u64::from(entries[i - 1].length))
                .ok_or("Malformed PMTiles directory")?
        } else {
            raw.checked_sub(1).ok_or("Malformed PMTiles directory")?
        };
    }
    Ok(entries)
}

/// A PMTiles v3 file opened for sequential tile reads.
pub struct PmtilesArchive {
    file: File,
    file_len: u64,
    pub header: PmtilesHeader,
}

impl PmtilesArchive {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|_| "The map could not be opened".to_string())?;
        let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut bytes = [0u8; PMTILES_HEADER_LEN];
        file.read_exact(&mut bytes).map_err(|_| "Not a PMTiles archive".to_string())?;
        let header = parse_pmtiles_header(&bytes)?;
        Ok(Self { file, file_len, header })
    }

    pub fn content(&self) -> MapContent {
        let h = &self.header;
        match h.tile_type {
            1 => {
                if let Compression::Other(_) = h.tile_compression {
                    return MapContent::Unsupported("Tile compression is not supported".to_string());
                }
                if let Compression::Other(_) = h.internal_compression {
                    return MapContent::Unsupported("Directory compression is not supported".to_string());
                }
                if h.min_zoom > h.max_zoom {
                    return MapContent::Unsupported("Invalid zoom range".to_string());
                }
                MapContent::Vector {
                    min_zoom: h.min_zoom,
                    max_zoom: h.max_zoom.min(MAX_ZOOM),
                }
            }
            2..=5 => MapContent::Raster,
            _ => MapContent::Unsupported("Unknown tile type".to_string()),
        }
    }

    fn read_at(&mut self, offset: u64, length: u64, cap: usize) -> Option<Vec<u8>> {
        let end = offset.checked_add(length)?;
        if length > cap as u64 || end > self.file_len {
            return None;
        }
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buf = vec![0u8; length as usize];
        self.file.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    fn directory(&mut self, offset: u64, length: u64) -> Result<Vec<Entry>, String> {
        let raw = self
            .read_at(offset, length, MAX_DIRECTORY_BYTES)
            .ok_or("PMTiles directory out of range")?;
        let data = decompress(&raw, Some(self.header.internal_compression), MAX_DIRECTORY_BYTES)
            .ok_or("PMTiles directory could not be decompressed")?;
        parse_directory(&data)
    }

    /// Raw (still compressed) tile bytes of an entry; `None` when the entry
    /// is out of range or larger than [`MAX_TILE_BYTES`].
    pub fn tile_data(&mut self, entry: &Entry) -> Option<Vec<u8>> {
        let offset = self.header.data_offset.checked_add(entry.offset)?;
        if entry.offset.checked_add(u64::from(entry.length))? > self.header.data_length {
            return None;
        }
        self.read_at(offset, u64::from(entry.length), MAX_TILE_BYTES)
    }

    /// Tile entries whose IDs intersect `lo..hi`, in tile-ID order, with run
    /// lengths clipped to that range.
    pub fn walk(&mut self, lo: u64, hi: u64) -> Result<PmtilesWalk, String> {
        let root = self.directory(self.header.root_offset, self.header.root_length)?;
        Ok(PmtilesWalk {
            lo,
            hi,
            stack: vec![Frame {
                entries: root,
                index: 0,
                end: u64::MAX,
            }],
        })
    }

    /// Number of tile entries (runs count once) intersecting `lo..hi`.
    pub fn count(&mut self, lo: u64, hi: u64, mut keep_going: impl FnMut() -> bool) -> Result<u64, String> {
        let mut walk = self.walk(lo, hi)?;
        let mut count = 0u64;
        while let Some(_entry) = walk.next(self)? {
            count += 1;
            if count % 4096 == 0 && !keep_going() {
                return Err("cancelled".to_string());
            }
        }
        Ok(count)
    }
}

struct Frame {
    entries: Vec<Entry>,
    index: usize,
    /// Exclusive upper tile-ID bound of this directory (from its parent).
    end: u64,
}

/// Lazy depth-first walk over directory entries (see [`PmtilesArchive::walk`]).
pub struct PmtilesWalk {
    lo: u64,
    hi: u64,
    stack: Vec<Frame>,
}

impl PmtilesWalk {
    pub fn next(&mut self, archive: &mut PmtilesArchive) -> Result<Option<Entry>, String> {
        loop {
            let Some(frame) = self.stack.last_mut() else {
                return Ok(None);
            };
            let Some(entry) = frame.entries.get(frame.index).copied() else {
                self.stack.pop();
                continue;
            };
            frame.index += 1;
            let next_id = frame
                .entries
                .get(frame.index)
                .map(|e| e.tile_id)
                .unwrap_or(frame.end)
                .min(frame.end);
            if entry.tile_id >= self.hi {
                // Entries are sorted: nothing further in this directory.
                frame.index = frame.entries.len();
                continue;
            }
            if entry.run_length == 0 {
                // Leaf directory covering [tile_id, next_id).
                if next_id <= self.lo {
                    continue;
                }
                if self.stack.len() >= MAX_DIRECTORY_DEPTH {
                    return Err("PMTiles directories nest too deeply".to_string());
                }
                let offset = archive
                    .header
                    .leaf_offset
                    .checked_add(entry.offset)
                    .ok_or("PMTiles directory out of range")?;
                if entry.offset.checked_add(u64::from(entry.length)).is_none_or(|end| end > archive.header.leaf_length) {
                    return Err("PMTiles directory out of range".to_string());
                }
                let entries = archive.directory(offset, u64::from(entry.length))?;
                self.stack.push(Frame {
                    entries,
                    index: 0,
                    end: next_id,
                });
                continue;
            }
            let start = entry.tile_id.max(self.lo);
            let end = entry.tile_id.saturating_add(u64::from(entry.run_length)).min(self.hi);
            if start >= end {
                continue;
            }
            return Ok(Some(Entry {
                tile_id: start,
                offset: entry.offset,
                length: entry.length,
                run_length: u32::try_from(end - start).unwrap_or(u32::MAX),
            }));
        }
    }
}

// ---------------------------------------------------------------------------
// MBTiles archive
// ---------------------------------------------------------------------------

pub struct MbtilesArchive {
    conn: Connection,
}

/// One MBTiles tile row (XYZ `y`, already flipped from TMS).
pub struct MbTile {
    pub x: u32,
    pub y: u32,
    /// TMS row, for the resume cursor.
    pub row: u32,
    /// `None` when the stored tile is larger than [`MAX_TILE_BYTES`].
    pub data: Option<Vec<u8>>,
}

impl MbtilesArchive {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|_| "The map could not be opened".to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(2)).ok();
        // Fails early for files that are not SQLite or have no tiles table.
        conn.query_row("SELECT 1 FROM tiles LIMIT 1", [], |_| Ok(()))
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(()),
                other => Err(other),
            })
            .map_err(|_| "Not an MBTiles archive".to_string())?;
        Ok(Self { conn })
    }

    fn metadata(&self, name: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM metadata WHERE name = ?1 LIMIT 1", [name], |row| row.get::<_, String>(0))
            .ok()
    }

    fn zoom_range(&self) -> Option<(u8, u8)> {
        let parse = |v: Option<String>| v.and_then(|s| s.trim().parse::<u8>().ok());
        if let (Some(min), Some(max)) = (parse(self.metadata("minzoom")), parse(self.metadata("maxzoom"))) {
            return Some((min, max));
        }
        self.conn
            .query_row("SELECT MIN(zoom_level), MAX(zoom_level) FROM tiles", [], |row| {
                Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .ok()
            .and_then(|(min, max)| Some((u8::try_from(min?).ok()?, u8::try_from(max?).ok()?)))
    }

    pub fn content(&self) -> MapContent {
        let format = self.metadata("format").map(|f| f.trim().to_ascii_lowercase());
        let vector = match format.as_deref() {
            Some("pbf" | "mvt" | "application/x-protobuf" | "application/vnd.mapbox-vector-tile") => true,
            Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "avif") => false,
            // Unknown or missing format: look at one tile.
            _ => {
                let sample: Option<Vec<u8>> = self
                    .conn
                    .query_row("SELECT substr(tile_data, 1, 16) FROM tiles LIMIT 1", [], |row| row.get(0))
                    .ok();
                match sample {
                    Some(bytes) if looks_like_image(&bytes) => false,
                    Some(_) => true,
                    None => return MapContent::Unsupported("The map has no tiles".to_string()),
                }
            }
        };
        if !vector {
            return MapContent::Raster;
        }
        match self.zoom_range() {
            Some((min, max)) if min <= max => MapContent::Vector {
                min_zoom: min,
                max_zoom: max.min(MAX_ZOOM),
            },
            _ => MapContent::Unsupported("The map has no tiles".to_string()),
        }
    }

    pub fn count(&self, z: u8) -> rusqlite::Result<u64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM tiles WHERE zoom_level = ?1", [i64::from(z)], |row| row.get::<_, i64>(0))
            .map(|n| n.max(0) as u64)
    }

    /// Up to `limit` tiles of zoom `z` at or after `(col, row)` in
    /// `(tile_column, tile_row)` order.
    pub fn tiles_from(&self, z: u8, col: u32, row: u32, limit: usize) -> rusqlite::Result<Vec<MbTile>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT tile_column, tile_row, CASE WHEN length(tile_data) <= ?5 THEN tile_data END \
             FROM tiles WHERE zoom_level = ?1 \
               AND (tile_column > ?2 OR (tile_column = ?2 AND tile_row >= ?3)) \
             ORDER BY tile_column, tile_row LIMIT ?4",
        )?;
        let n = 1i64 << z.min(MAX_ZOOM);
        let rows = stmt.query_map(
            params![i64::from(z), i64::from(col), i64::from(row), limit as i64, MAX_TILE_BYTES as i64],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<Vec<u8>>>(2)?)),
        )?;
        let mut out = Vec::new();
        for item in rows {
            let (x, tms_row, data) = item?;
            // Rows outside the zoom's grid are skipped (but still advance the
            // cursor via `row`).
            let valid = (0..n).contains(&x) && (0..n).contains(&tms_row);
            let (Ok(x32), Ok(row32)) = (u32::try_from(x), u32::try_from(tms_row)) else {
                continue;
            };
            out.push(MbTile {
                x: x32,
                y: if valid { (n - 1 - tms_row) as u32 } else { u32::MAX },
                row: row32,
                data: if valid { data } else { None },
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_ids_follow_the_pmtiles_spec() {
        // Values from the PMTiles reference implementation.
        assert_eq!(zxy_to_tile_id(0, 0, 0), Some(0));
        assert_eq!(zxy_to_tile_id(1, 0, 0), Some(1));
        assert_eq!(zxy_to_tile_id(1, 0, 1), Some(2));
        assert_eq!(zxy_to_tile_id(1, 1, 1), Some(3));
        assert_eq!(zxy_to_tile_id(1, 1, 0), Some(4));
        assert_eq!(zxy_to_tile_id(2, 0, 0), Some(5));
        assert_eq!(zxy_to_tile_id(1, 2, 0), None);
        for z in [0u8, 1, 3, 7, 12, 14, 20] {
            let n = 1u32 << z;
            for (x, y) in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1), (n / 3, n / 2)] {
                let id = zxy_to_tile_id(z, x, y).unwrap();
                assert!(id >= zoom_first_id(z) && id < zoom_first_id(z + 1));
                assert_eq!(tile_id_to_zxy(id), Some((z, x, y)), "z{z} {x}/{y}");
            }
        }
        assert_eq!(tile_id_to_zxy(u64::MAX), None);
    }

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn tile_coordinates_map_to_wgs84_in_both_hemispheres() {
        // Tile corners.
        let (lon, lat) = tile_to_lon_lat(0, 0, 0, 0.0, 0.0, 4096).unwrap();
        assert!(close(lon, -180.0, 1e-9) && close(lat, MAX_LATITUDE, 1e-9));
        let (lon, lat) = tile_to_lon_lat(1, 1, 1, 0.0, 0.0, 4096).unwrap();
        assert!(close(lon, 0.0, 1e-9) && close(lat, 0.0, 1e-9));
        // Round trips at several zooms: Stockholm, Sydney, Santiago, Tokyo, Reykjavik.
        for (lon, lat) in [(18.0686, 59.3293), (151.2093, -33.8688), (-70.6693, -33.4489), (139.6917, 35.6895), (-21.9426, 64.1466)] {
            for z in [0u8, 3, 7, 10, 14, 18] {
                let (x, y, fx, fy) = lon_lat_to_tile(z, lon, lat);
                let (lon2, lat2) = tile_to_lon_lat(z, x, y, fx * 4096.0, fy * 4096.0, 4096).unwrap();
                assert!(close(lon, lon2, 1e-6) && close(lat, lat2, 1e-6), "z{z}: {lon},{lat} → {lon2},{lat2}");
                // Quantised to the extent grid the error stays below one unit.
                let unit_deg = 360.0 / f64::from(1u32 << z) / 4096.0;
                let (lon3, lat3) =
                    tile_to_lon_lat(z, x, y, (fx * 4096.0).round(), (fy * 4096.0).round(), 4096).unwrap();
                assert!(close(lon, lon3, unit_deg) && close(lat, lat3, unit_deg), "z{z} quantised");
            }
        }
        // Buffer positions and extreme values are clamped, invalid input rejected.
        let (lon, lat) = tile_to_lon_lat(0, 0, 0, -1e12, -1e12, 4096).unwrap();
        assert!(close(lon, -180.0, 1e-9) && close(lat, MAX_LATITUDE, 1e-9));
        assert!(tile_to_lon_lat(0, 0, 0, 1e300, 1e300, 4096).is_some());
        assert_eq!(tile_to_lon_lat(2, 4, 0, 0.0, 0.0, 4096), None);
        assert_eq!(tile_to_lon_lat(2, 0, 0, 0.0, 0.0, 0), None);
        assert_eq!(tile_to_lon_lat(40, 0, 0, 0.0, 0.0, 4096), None);
        assert_eq!(tile_to_lon_lat(2, 0, 0, f64::NAN, 0.0, 4096), None);
    }

    #[test]
    fn decompression_is_capped_and_sniffed() {
        use std::io::Write;
        let payload = vec![7u8; 10_000];
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&payload).unwrap();
        let gz = gz.finish().unwrap();
        assert_eq!(sniff_compression(&gz), Compression::Gzip);
        assert_eq!(decompress(&gz, None, 10_000).as_deref(), Some(&payload[..]));
        assert_eq!(decompress(&gz, None, 9_999), None, "decompression bombs are cut off");
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(&payload).unwrap();
        let zlib = zlib.finish().unwrap();
        assert_eq!(sniff_compression(&zlib), Compression::Gzip);
        assert_eq!(decompress(&zlib, None, 20_000).as_deref(), Some(&payload[..]));
        let zst = zstd::encode_all(&payload[..], 3).unwrap();
        assert_eq!(decompress(&zst, None, 20_000).as_deref(), Some(&payload[..]));
        assert_eq!(decompress(&[0x1f, 0x8b, 0, 1, 2], None, 100), None);
        assert_eq!(decompress(b"raw", Some(Compression::Other(3)), 100), None);
        assert_eq!(sniff_compression(&[0x1a, 0x05]), Compression::None);
    }

    #[test]
    fn directories_parse_and_reject_garbage() {
        // Two entries, offsets 0 and implicit 10.
        let mut dir = Vec::new();
        for v in [2u64, 5, 1, 1, 0, 10, 20, 1, 0] {
            super::super::mvt::encode::varint(v, &mut dir);
        }
        let entries = parse_directory(&dir).unwrap();
        assert_eq!(
            entries,
            vec![
                Entry { tile_id: 5, offset: 0, length: 10, run_length: 1 },
                Entry { tile_id: 6, offset: 10, length: 20, run_length: 0 }
            ]
        );
        assert!(parse_directory(&[0xff, 0xff, 0xff, 0xff, 0x0f]).is_err(), "huge count");
        assert!(parse_directory(&dir[..dir.len() - 1]).is_err(), "truncated");
        assert!(parse_directory(&[]).is_err());
        let mut overflow = Vec::new();
        for v in [2u64, u64::MAX, 5] {
            super::super::mvt::encode::varint(v, &mut overflow);
        }
        assert!(parse_directory(&overflow).is_err());
        assert!(parse_pmtiles_header(b"PMTiles").is_err());
        let mut header = vec![0u8; 127];
        header[..7].copy_from_slice(b"PMTiles");
        header[7] = 2;
        assert!(parse_pmtiles_header(&header).is_err());
    }
}
