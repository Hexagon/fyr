//! Minimal, bounds-checked decoder for Mapbox Vector Tiles (MVT 2.x).
//!
//! Only what the place indexer needs: layer names, extents, keys/values and
//! per-feature tags and geometry commands. Hand-written instead of pulling
//! in a protobuf code generator (prost + build script) or an unmaintained
//! MVT crate: the format is five small messages, and a decoder that only
//! borrows from the input, never allocates more than the input implies and
//! returns an error on any malformed byte is easy to audit and fuzz here.
//!
//! Every read is bounds-checked; varints longer than 10 bytes or overflowing
//! 64 bits, unknown wire types, groups and lengths past the end of a message
//! are errors. Invalid UTF-8 in keys or string values makes that key/value
//! unusable but does not fail the layer.

use std::fmt;

/// Decoding failure. Tiles that fail to decode are skipped by the indexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    VarintOverflow,
    WireType(u8),
    Geometry,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Truncated => f.write_str("truncated protobuf message"),
            DecodeError::VarintOverflow => f.write_str("varint overflow"),
            DecodeError::WireType(wt) => write!(f, "unsupported wire type {wt}"),
            DecodeError::Geometry => f.write_str("malformed geometry"),
        }
    }
}

type Result<T> = std::result::Result<T, DecodeError>;

// ---------------------------------------------------------------------------
// Protobuf wire format
// ---------------------------------------------------------------------------

/// A protobuf field value as found on the wire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wire<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// Sequential reader over one protobuf message.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub fn varint(&mut self) -> Result<u64> {
        let mut value: u64 = 0;
        for i in 0..10 {
            let byte = *self.buf.get(self.pos).ok_or(DecodeError::Truncated)?;
            self.pos += 1;
            if i == 9 && byte > 1 {
                return Err(DecodeError::VarintOverflow);
            }
            value |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DecodeError::VarintOverflow)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).ok_or(DecodeError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// Next `(field number, value)`, or `None` at the end of the message.
    pub fn field(&mut self) -> Result<Option<(u64, Wire<'a>)>> {
        if self.is_empty() {
            return Ok(None);
        }
        let key = self.varint()?;
        let number = key >> 3;
        let value = match (key & 7) as u8 {
            0 => Wire::Varint(self.varint()?),
            1 => {
                let bytes = self.take(8)?;
                Wire::Fixed64(u64::from_le_bytes(bytes.try_into().map_err(|_| DecodeError::Truncated)?))
            }
            2 => {
                let len = usize::try_from(self.varint()?).map_err(|_| DecodeError::Truncated)?;
                Wire::Bytes(self.take(len)?)
            }
            5 => {
                let bytes = self.take(4)?;
                Wire::Fixed32(u32::from_le_bytes(bytes.try_into().map_err(|_| DecodeError::Truncated)?))
            }
            other => return Err(DecodeError::WireType(other)),
        };
        Ok(Some((number, value)))
    }
}

/// Decode a packed `repeated uint32` field into `out`.
fn packed_u32(bytes: &[u8], out: &mut Vec<u32>) -> Result<()> {
    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        out.push(reader.varint()? as u32);
    }
    Ok(())
}

pub fn zigzag(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

// ---------------------------------------------------------------------------
// MVT messages
// ---------------------------------------------------------------------------

/// A feature attribute value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    Str(&'a str),
    Float(f32),
    Double(f64),
    Int(i64),
    Uint(u64),
    Bool(bool),
    /// Unparseable or unknown value types.
    Invalid,
}

impl<'a> Value<'a> {
    fn decode(bytes: &'a [u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let mut value = Value::Invalid;
        while let Some((number, wire)) = reader.field()? {
            value = match (number, wire) {
                (1, Wire::Bytes(b)) => std::str::from_utf8(b).map(Value::Str).unwrap_or(Value::Invalid),
                (2, Wire::Fixed32(v)) => Value::Float(f32::from_bits(v)),
                (3, Wire::Fixed64(v)) => Value::Double(f64::from_bits(v)),
                (4, Wire::Varint(v)) => Value::Int(v as i64),
                (5, Wire::Varint(v)) => Value::Uint(v),
                (6, Wire::Varint(v)) => Value::Int(((v >> 1) as i64) ^ -((v & 1) as i64)),
                (7, Wire::Varint(v)) => Value::Bool(v != 0),
                _ => value,
            };
        }
        Ok(value)
    }

    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Numeric value (numbers, or strings that parse as numbers).
    pub fn as_f64(&self) -> Option<f64> {
        let v = match *self {
            Value::Float(f) => f64::from(f),
            Value::Double(d) => d,
            Value::Int(i) => i as f64,
            Value::Uint(u) => u as f64,
            Value::Str(s) => s.trim().parse::<f64>().ok()?,
            Value::Bool(_) | Value::Invalid => return None,
        };
        v.is_finite().then_some(v)
    }
}

/// MVT geometry types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeomType {
    Unknown,
    Point,
    LineString,
    Polygon,
}

impl GeomType {
    fn from_u64(value: u64) -> Self {
        match value {
            1 => GeomType::Point,
            2 => GeomType::LineString,
            3 => GeomType::Polygon,
            _ => GeomType::Unknown,
        }
    }
}

/// One decoded feature: attribute index pairs and raw geometry commands.
#[derive(Debug, Clone, Default)]
pub struct Feature {
    pub geom_type: Option<GeomType>,
    pub tags: Vec<u32>,
    pub geometry: Vec<u32>,
}

/// A layer whose keys and values are decoded; features stay raw until
/// [`Layer::feature`] is called.
#[derive(Debug, Clone)]
pub struct Layer<'a> {
    pub name: &'a str,
    pub extent: u32,
    pub keys: Vec<Option<&'a str>>,
    pub values: Vec<Value<'a>>,
    features: Vec<&'a [u8]>,
}

impl<'a> Layer<'a> {
    pub fn feature_count(&self) -> usize {
        self.features.len()
    }

    pub fn feature(&self, index: usize) -> Result<Feature> {
        let bytes = self.features.get(index).ok_or(DecodeError::Truncated)?;
        let mut reader = Reader::new(bytes);
        let mut feature = Feature::default();
        while let Some((number, wire)) = reader.field()? {
            match (number, wire) {
                (2, Wire::Bytes(b)) => packed_u32(b, &mut feature.tags)?,
                (2, Wire::Varint(v)) => feature.tags.push(v as u32),
                (3, Wire::Varint(v)) => feature.geom_type = Some(GeomType::from_u64(v)),
                (4, Wire::Bytes(b)) => packed_u32(b, &mut feature.geometry)?,
                (4, Wire::Varint(v)) => feature.geometry.push(v as u32),
                _ => {}
            }
        }
        Ok(feature)
    }

    /// Value of attribute `key` on `feature`, if present and well-formed.
    pub fn attr(&self, feature: &Feature, key: &str) -> Option<Value<'a>> {
        self.attrs(feature).find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// All well-formed `(key, value)` attributes of `feature`.
    pub fn attrs<'f>(&'f self, feature: &'f Feature) -> impl Iterator<Item = (&'a str, Value<'a>)> + 'f {
        feature.tags.chunks_exact(2).filter_map(move |pair| {
            let key = (*self.keys.get(pair[0] as usize)?)?;
            let value = *self.values.get(pair[1] as usize)?;
            (value != Value::Invalid).then_some((key, value))
        })
    }
}

/// Decode the layers of a tile whose name satisfies `wanted`. Other layers
/// are skipped without decoding their contents.
pub fn decode_layers<'a>(tile: &'a [u8], wanted: impl Fn(&str) -> bool) -> Result<Vec<Layer<'a>>> {
    let mut reader = Reader::new(tile);
    let mut layers = Vec::new();
    while let Some((number, wire)) = reader.field()? {
        if let (3, Wire::Bytes(bytes)) = (number, wire) {
            if let Some(layer) = decode_layer(bytes, &wanted)? {
                layers.push(layer);
            }
        }
    }
    Ok(layers)
}

fn decode_layer<'a>(bytes: &'a [u8], wanted: &impl Fn(&str) -> bool) -> Result<Option<Layer<'a>>> {
    // The name may come after the features: find it first.
    let mut name: Option<&'a str> = None;
    let mut reader = Reader::new(bytes);
    while let Some((number, wire)) = reader.field()? {
        if let (1, Wire::Bytes(b)) = (number, wire) {
            name = std::str::from_utf8(b).ok();
        }
    }
    let Some(name) = name.filter(|n| wanted(n)) else {
        return Ok(None);
    };
    let mut layer = Layer {
        name,
        extent: 4096,
        keys: Vec::new(),
        values: Vec::new(),
        features: Vec::new(),
    };
    let mut reader = Reader::new(bytes);
    while let Some((number, wire)) = reader.field()? {
        match (number, wire) {
            (2, Wire::Bytes(b)) => layer.features.push(b),
            (3, Wire::Bytes(b)) => layer.keys.push(std::str::from_utf8(b).ok()),
            (4, Wire::Bytes(b)) => layer.values.push(Value::decode(b)?),
            (5, Wire::Varint(v)) => layer.extent = u32::try_from(v).unwrap_or(0),
            _ => {}
        }
    }
    if layer.extent == 0 {
        return Err(DecodeError::Geometry);
    }
    Ok(Some(layer))
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// At most this many vertices are decoded per feature.
pub const MAX_VERTICES: usize = 100_000;

/// The parts (point sets, lines or rings) of a geometry in tile-local
/// integer coordinates. Coordinates may lie outside `0..extent` (buffer).
pub fn decode_geometry(commands: &[u32]) -> Result<Vec<Vec<(i64, i64)>>> {
    let mut parts: Vec<Vec<(i64, i64)>> = Vec::new();
    let (mut x, mut y) = (0i64, 0i64);
    let mut vertices = 0usize;
    let mut i = 0usize;
    while i < commands.len() {
        let command = commands[i];
        i += 1;
        let id = command & 7;
        let count = (command >> 3) as usize;
        match id {
            1 | 2 => {
                let needed = count.checked_mul(2).ok_or(DecodeError::Geometry)?;
                if needed > commands.len() - i {
                    return Err(DecodeError::Geometry);
                }
                if id == 2 && parts.is_empty() {
                    return Err(DecodeError::Geometry);
                }
                for _ in 0..count {
                    x = x.saturating_add(i64::from(zigzag(commands[i])));
                    y = y.saturating_add(i64::from(zigzag(commands[i + 1])));
                    i += 2;
                    vertices += 1;
                    if vertices > MAX_VERTICES {
                        return Err(DecodeError::Geometry);
                    }
                    // Each MoveTo vertex starts a new part (multipoints too).
                    if id == 1 {
                        parts.push(Vec::new());
                    }
                    if let Some(part) = parts.last_mut() {
                        part.push((x, y));
                    }
                }
            }
            7 => {
                if parts.is_empty() {
                    return Err(DecodeError::Geometry);
                }
            }
            _ => return Err(DecodeError::Geometry),
        }
    }
    Ok(parts)
}

/// A single representative point for a feature, in tile-local coordinates:
/// the first point of a (multi)point, the middle vertex of a line's first
/// part, or the vertex mean of a polygon's outer ring.
pub fn representative_point(geom_type: GeomType, commands: &[u32]) -> Result<Option<(f64, f64)>> {
    let parts = decode_geometry(commands)?;
    let Some(first) = parts.iter().find(|p| !p.is_empty()) else {
        return Ok(None);
    };
    let point = match geom_type {
        GeomType::Point | GeomType::Unknown => first[0],
        GeomType::LineString => first[first.len() / 2],
        GeomType::Polygon => {
            let n = first.len() as f64;
            let (sx, sy) = first
                .iter()
                .fold((0f64, 0f64), |(sx, sy), (x, y)| (sx + *x as f64, sy + *y as f64));
            return Ok(Some((sx / n, sy / n)));
        }
    };
    Ok(Some((point.0 as f64, point.1 as f64)))
}

#[cfg(test)]
pub mod encode {
    //! A tiny MVT *encoder* for tests and fixtures.

    pub fn varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    pub fn key(number: u64, wire: u64, out: &mut Vec<u8>) {
        varint((number << 3) | wire, out);
    }

    pub fn bytes_field(number: u64, bytes: &[u8], out: &mut Vec<u8>) {
        key(number, 2, out);
        varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }

    pub fn varint_field(number: u64, value: u64, out: &mut Vec<u8>) {
        key(number, 0, out);
        varint(value, out);
    }

    pub fn zigzag(value: i32) -> u32 {
        ((value << 1) ^ (value >> 31)) as u32
    }

    #[derive(Debug, Clone)]
    pub enum TestValue {
        Str(String),
        Int(i64),
        Double(f64),
    }

    impl From<&str> for TestValue {
        fn from(value: &str) -> Self {
            TestValue::Str(value.to_string())
        }
    }

    impl From<i64> for TestValue {
        fn from(value: i64) -> Self {
            TestValue::Int(value)
        }
    }

    #[derive(Debug, Clone)]
    pub struct TestFeature {
        /// 1 point, 2 line, 3 polygon
        pub geom_type: u64,
        /// Absolute tile-local vertices of one part.
        pub points: Vec<(i32, i32)>,
        pub attrs: Vec<(String, TestValue)>,
    }

    impl TestFeature {
        pub fn point(x: i32, y: i32, attrs: &[(&str, TestValue)]) -> Self {
            Self {
                geom_type: 1,
                points: vec![(x, y)],
                attrs: attrs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            }
        }

        pub fn line(points: &[(i32, i32)], attrs: &[(&str, TestValue)]) -> Self {
            Self {
                geom_type: 2,
                points: points.to_vec(),
                attrs: attrs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            }
        }

        pub fn polygon(ring: &[(i32, i32)], attrs: &[(&str, TestValue)]) -> Self {
            Self {
                geom_type: 3,
                points: ring.to_vec(),
                attrs: attrs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
            }
        }
    }

    pub fn geometry(geom_type: u64, points: &[(i32, i32)]) -> Vec<u32> {
        let mut out = Vec::new();
        let (mut cx, mut cy) = (0i32, 0i32);
        let mut push = |out: &mut Vec<u32>, (x, y): (i32, i32)| {
            out.push(zigzag(x - cx));
            out.push(zigzag(y - cy));
            cx = x;
            cy = y;
        };
        if points.is_empty() {
            return out;
        }
        out.push((1 << 3) | 1);
        push(&mut out, points[0]);
        if points.len() > 1 {
            let rest = &points[1..];
            out.push(((rest.len() as u32) << 3) | 2);
            for p in rest {
                push(&mut out, *p);
            }
        }
        if geom_type == 3 {
            out.push((1 << 3) | 7);
        }
        out
    }

    fn packed(values: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        for v in values {
            varint(u64::from(*v), &mut out);
        }
        out
    }

    pub fn layer(name: &str, extent: u32, features: &[TestFeature]) -> Vec<u8> {
        let mut keys: Vec<String> = Vec::new();
        let mut values: Vec<Vec<u8>> = Vec::new();
        let mut out = Vec::new();
        varint_field(15, 2, &mut out);
        bytes_field(1, name.as_bytes(), &mut out);
        for feature in features {
            let mut tags = Vec::new();
            for (k, v) in &feature.attrs {
                let ki = keys.iter().position(|x| x == k).unwrap_or_else(|| {
                    keys.push(k.clone());
                    keys.len() - 1
                });
                let mut encoded = Vec::new();
                match v {
                    TestValue::Str(s) => bytes_field(1, s.as_bytes(), &mut encoded),
                    TestValue::Int(i) => varint_field(4, *i as u64, &mut encoded),
                    TestValue::Double(d) => {
                        key(3, 1, &mut encoded);
                        encoded.extend_from_slice(&d.to_bits().to_le_bytes());
                    }
                }
                let vi = values.iter().position(|x| *x == encoded).unwrap_or_else(|| {
                    values.push(encoded);
                    values.len() - 1
                });
                tags.push(ki as u32);
                tags.push(vi as u32);
            }
            let mut f = Vec::new();
            bytes_field(2, &packed(&tags), &mut f);
            varint_field(3, feature.geom_type, &mut f);
            bytes_field(4, &packed(&geometry(feature.geom_type, &feature.points)), &mut f);
            bytes_field(2, &f, &mut out);
        }
        for k in &keys {
            bytes_field(3, k.as_bytes(), &mut out);
        }
        for v in &values {
            bytes_field(4, v, &mut out);
        }
        varint_field(5, u64::from(extent), &mut out);
        out
    }

    pub fn tile(layers: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for layer in layers {
            bytes_field(3, layer, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::encode::{self, TestFeature, TestValue};
    use super::*;

    #[test]
    fn varints_decode_and_reject_overflow() {
        assert_eq!(Reader::new(&[0x96, 0x01]).varint(), Ok(150));
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]).varint(),
            Ok(u64::MAX)
        );
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02]).varint(),
            Err(DecodeError::VarintOverflow)
        );
        assert_eq!(Reader::new(&[0xff; 11]).varint(), Err(DecodeError::VarintOverflow));
        assert_eq!(Reader::new(&[0x80]).varint(), Err(DecodeError::Truncated));
        assert_eq!(Reader::new(&[]).varint(), Err(DecodeError::Truncated));
    }

    #[test]
    fn zigzag_round_trips() {
        for v in [0, 1, -1, 2, -2, 4095, -4096, i32::MAX, i32::MIN] {
            assert_eq!(zigzag(encode::zigzag(v)), v);
        }
    }

    #[test]
    fn decodes_layers_attributes_and_points() {
        let layer = encode::layer(
            "place",
            4096,
            &[
                TestFeature::point(100, 200, &[("name", "Malmö".into()), ("class", "city".into()), ("population", TestValue::Int(350_000))]),
                TestFeature::point(5, 6, &[("name", "Lund".into()), ("rank", TestValue::Double(4.0))]),
            ],
        );
        let other = encode::layer("roads", 4096, &[TestFeature::line(&[(0, 0), (10, 10)], &[])]);
        let tile = encode::tile(&[other, layer]);
        let layers = decode_layers(&tile, |n| n == "place").unwrap();
        assert_eq!(layers.len(), 1);
        let layer = &layers[0];
        assert_eq!((layer.name, layer.extent, layer.feature_count()), ("place", 4096, 2));
        let first = layer.feature(0).unwrap();
        assert_eq!(layer.attr(&first, "name").and_then(|v| v.as_str()), Some("Malmö"));
        assert_eq!(layer.attr(&first, "population").and_then(|v| v.as_f64()), Some(350_000.0));
        assert_eq!(first.geom_type, Some(GeomType::Point));
        assert_eq!(representative_point(GeomType::Point, &first.geometry).unwrap(), Some((100.0, 200.0)));
        let second = layer.feature(1).unwrap();
        assert_eq!(layer.attr(&second, "rank").and_then(|v| v.as_f64()), Some(4.0));
    }

    #[test]
    fn representative_points_for_lines_and_polygons() {
        let line = encode::geometry(2, &[(0, 0), (10, 0), (20, 0), (30, 0), (40, 0)]);
        assert_eq!(representative_point(GeomType::LineString, &line).unwrap(), Some((20.0, 0.0)));
        let ring = encode::geometry(3, &[(0, 0), (10, 0), (10, 10), (0, 10)]);
        assert_eq!(representative_point(GeomType::Polygon, &ring).unwrap(), Some((5.0, 5.0)));
        // Negative (buffer) coordinates survive.
        let point = encode::geometry(1, &[(-64, 4100)]);
        assert_eq!(representative_point(GeomType::Point, &point).unwrap(), Some((-64.0, 4100.0)));
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        // Truncated length-delimited field.
        assert!(decode_layers(&[0x1a, 0x10, 0x01], |_| true).is_err());
        // Group wire type.
        assert_eq!(decode_layers(&[0x1b], |_| true).unwrap_err(), DecodeError::WireType(3));
        // Geometry: LineTo before MoveTo, counts past the end, unknown command.
        assert!(decode_geometry(&[(1 << 3) | 2, 2, 2]).is_err());
        assert!(decode_geometry(&[(1000 << 3) | 1, 2, 2]).is_err());
        assert!(decode_geometry(&[(1 << 3) | 5, 0, 0]).is_err());
        assert!(decode_geometry(&[u32::MAX]).is_err());
        assert_eq!(decode_geometry(&[]).unwrap(), Vec::<Vec<(i64, i64)>>::new());
        // Extreme deltas saturate instead of overflowing.
        let mut commands = vec![((MAX_VERTICES as u32 - 1) << 3) | 1];
        for _ in 0..MAX_VERTICES - 1 {
            commands.push(u32::MAX - 1);
            commands.push(u32::MAX);
        }
        assert!(decode_geometry(&commands).is_ok());
        // Too many vertices.
        let mut commands = vec![((MAX_VERTICES as u32 + 1) << 3) | 1];
        commands.extend(std::iter::repeat(0).take((MAX_VERTICES + 1) * 2));
        assert_eq!(decode_geometry(&commands).unwrap_err(), DecodeError::Geometry);
        // Zero extent.
        let mut layer = Vec::new();
        encode::bytes_field(1, b"place", &mut layer);
        encode::varint_field(5, 0, &mut layer);
        assert!(decode_layers(&encode::tile(&[layer]), |_| true).is_err());
    }

    #[test]
    fn out_of_range_tag_indexes_and_bad_utf8_are_ignored() {
        let mut feature = Vec::new();
        encode::bytes_field(2, &[0, 0, 7, 9, 1, 1], &mut feature);
        encode::varint_field(3, 1, &mut feature);
        encode::bytes_field(4, &[9, 2, 2], &mut feature);
        let mut layer = Vec::new();
        encode::bytes_field(1, b"place", &mut layer);
        encode::bytes_field(2, &feature, &mut layer);
        encode::bytes_field(3, b"name", &mut layer);
        encode::bytes_field(3, &[0xff, 0xfe], &mut layer);
        let mut value = Vec::new();
        encode::bytes_field(1, b"Ystad", &mut value);
        encode::bytes_field(4, &value, &mut layer);
        let tile = encode::tile(&[layer]);
        let layers = decode_layers(&tile, |_| true).unwrap();
        let f = layers[0].feature(0).unwrap();
        let attrs: Vec<_> = layers[0].attrs(&f).collect();
        assert_eq!(attrs, vec![("name", Value::Str("Ystad"))]);
    }

    #[test]
    fn random_bytes_never_panic() {
        // Deterministic pseudo-random fuzzing of the whole decode path.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let base = encode::tile(&[encode::layer(
            "place",
            4096,
            &[TestFeature::point(1, 2, &[("name", "A".into())]), TestFeature::line(&[(0, 0), (5, 5)], &[("name", "B".into())])],
        )]);
        for round in 0..20_000 {
            let mut bytes = base.clone();
            let flips = 1 + (next() % 4) as usize;
            for _ in 0..flips {
                let at = (next() as usize) % bytes.len();
                bytes[at] = next() as u8;
            }
            if round % 3 == 0 {
                bytes.truncate((next() as usize) % bytes.len());
            }
            if let Ok(layers) = decode_layers(&bytes, |_| true) {
                for layer in &layers {
                    for i in 0..layer.feature_count() {
                        if let Ok(f) = layer.feature(i) {
                            let _ = layer.attrs(&f).count();
                            let _ = representative_point(f.geom_type.unwrap_or(GeomType::Unknown), &f.geometry);
                        }
                    }
                }
            }
        }
    }
}
