#![allow(dead_code)]

use std::fmt;

use serde::Serialize;

const ENCODE_FLAG_64: u32 = 1 << 16;
const ENCODE_FLAG_OBJECT_AS_ID: u32 = 1 << 16;
const TYPE_MASK: u32 = 0xFFFF;

const TYPE_NIL: u32 = 0;
const TYPE_BOOL: u32 = 1;
const TYPE_INT: u32 = 2;
const TYPE_FLOAT: u32 = 3;
const TYPE_STRING: u32 = 4;
const TYPE_VECTOR2: u32 = 5;
const TYPE_VECTOR2I: u32 = 6;
const TYPE_RECT2: u32 = 7;
const TYPE_RECT2I: u32 = 8;
const TYPE_VECTOR3: u32 = 9;
const TYPE_VECTOR3I: u32 = 10;
const TYPE_TRANSFORM2D: u32 = 11;
const TYPE_VECTOR4: u32 = 12;
const TYPE_VECTOR4I: u32 = 13;
const TYPE_PLANE: u32 = 14;
const TYPE_QUATERNION: u32 = 15;
const TYPE_AABB: u32 = 16;
const TYPE_BASIS: u32 = 17;
const TYPE_TRANSFORM3D: u32 = 18;
const TYPE_PROJECTION: u32 = 19;
const TYPE_COLOR: u32 = 20;
const TYPE_STRING_NAME: u32 = 21;
const TYPE_NODE_PATH: u32 = 22;
const TYPE_RID: u32 = 23;
const TYPE_OBJECT: u32 = 24;
const TYPE_CALLABLE: u32 = 25;
const TYPE_SIGNAL: u32 = 26;
const TYPE_DICTIONARY: u32 = 27;
const TYPE_ARRAY: u32 = 28;
const TYPE_PACKED_BYTE_ARRAY: u32 = 29;
const TYPE_PACKED_INT32_ARRAY: u32 = 30;
const TYPE_PACKED_INT64_ARRAY: u32 = 31;
const TYPE_PACKED_FLOAT32_ARRAY: u32 = 32;
const TYPE_PACKED_FLOAT64_ARRAY: u32 = 33;
const TYPE_PACKED_STRING_ARRAY: u32 = 34;
const TYPE_PACKED_VECTOR2_ARRAY: u32 = 35;
const TYPE_PACKED_VECTOR3_ARRAY: u32 = 36;
const TYPE_PACKED_COLOR_ARRAY: u32 = 37;
const TYPE_PACKED_VECTOR4_ARRAY: u32 = 38;

/// A Godot Variant value, representing any of the 39 built-in types.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum GodotVariant {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Vector2(f64, f64),
    Vector2i(i32, i32),
    Rect2(f64, f64, f64, f64),
    Rect2i(i32, i32, i32, i32),
    Vector3(f64, f64, f64),
    Vector3i(i32, i32, i32),
    Transform2D([f64; 6]),
    Vector4(f64, f64, f64, f64),
    Vector4i(i32, i32, i32, i32),
    Plane(f64, f64, f64, f64),
    Quaternion(f64, f64, f64, f64),
    Aabb([f64; 6]),
    Basis([f64; 9]),
    Transform3D([f64; 12]),
    Projection([f64; 16]),
    Color(f32, f32, f32, f32),
    StringName(String),
    NodePath(String),
    Rid(u64),
    Object {
        class: String,
        properties: Vec<(String, GodotVariant)>,
    },
    ObjectId(u64),
    Callable,
    Signal {
        name: String,
        object_id: u64,
    },
    Dictionary(Vec<(GodotVariant, GodotVariant)>),
    Array(Vec<GodotVariant>),
    PackedByteArray(Vec<u8>),
    PackedInt32Array(Vec<i32>),
    PackedInt64Array(Vec<i64>),
    PackedFloat32Array(Vec<f32>),
    PackedFloat64Array(Vec<f64>),
    PackedStringArray(Vec<String>),
    PackedVector2Array(Vec<(f32, f32)>),
    PackedVector3Array(Vec<(f32, f32, f32)>),
    PackedColorArray(Vec<(f32, f32, f32, f32)>),
    PackedVector4Array(Vec<(f32, f32, f32, f32)>),
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl fmt::Display for GodotVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "null"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::String(v) | Self::StringName(v) => write!(f, "\"{v}\""),
            Self::Vector2(x, y) => write!(f, "Vector2({x}, {y})"),
            Self::Vector2i(x, y) => write!(f, "Vector2i({x}, {y})"),
            Self::Rect2(x, y, w, h) => write!(f, "Rect2({x}, {y}, {w}, {h})"),
            Self::Rect2i(x, y, w, h) => write!(f, "Rect2i({x}, {y}, {w}, {h})"),
            Self::Vector3(x, y, z) => write!(f, "Vector3({x}, {y}, {z})"),
            Self::Vector3i(x, y, z) => write!(f, "Vector3i({x}, {y}, {z})"),
            Self::Transform2D(v) => write!(
                f,
                "Transform2D({}, {}, {}, {}, {}, {})",
                v[0], v[1], v[2], v[3], v[4], v[5]
            ),
            Self::Vector4(x, y, z, w) => write!(f, "Vector4({x}, {y}, {z}, {w})"),
            Self::Vector4i(x, y, z, w) => write!(f, "Vector4i({x}, {y}, {z}, {w})"),
            Self::Plane(a, b, c, d) => write!(f, "Plane({a}, {b}, {c}, {d})"),
            Self::Quaternion(x, y, z, w) => write!(f, "Quaternion({x}, {y}, {z}, {w})"),
            Self::Aabb(v) => write!(
                f,
                "AABB({}, {}, {}, {}, {}, {})",
                v[0], v[1], v[2], v[3], v[4], v[5]
            ),
            Self::Basis(v) => {
                write!(f, "Basis(")?;
                for (i, val) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{val}")?;
                }
                write!(f, ")")
            }
            Self::Transform3D(v) => {
                write!(f, "Transform3D(")?;
                for (i, val) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{val}")?;
                }
                write!(f, ")")
            }
            Self::Projection(v) => {
                write!(f, "Projection(")?;
                for (i, val) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{val}")?;
                }
                write!(f, ")")
            }
            Self::Color(r, g, b, a) => write!(f, "Color({r}, {g}, {b}, {a})"),
            Self::NodePath(v) => write!(f, "NodePath(\"{v}\")"),
            Self::Rid(v) => write!(f, "RID({v})"),
            Self::Object { class, properties } => {
                write!(f, "{class}{{")?;
                for (i, (name, val)) in properties.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {val}")?;
                }
                write!(f, "}}")
            }
            Self::ObjectId(id) => write!(f, "Object#{id}"),
            Self::Callable => write!(f, "Callable()"),
            Self::Signal { name, object_id } => write!(f, "Signal({name}, #{object_id})"),
            Self::Dictionary(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Self::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Self::PackedByteArray(v) => write!(f, "PackedByteArray(size={})", v.len()),
            Self::PackedInt32Array(v) => write!(f, "PackedInt32Array(size={})", v.len()),
            Self::PackedInt64Array(v) => write!(f, "PackedInt64Array(size={})", v.len()),
            Self::PackedFloat32Array(v) => write!(f, "PackedFloat32Array(size={})", v.len()),
            Self::PackedFloat64Array(v) => write!(f, "PackedFloat64Array(size={})", v.len()),
            Self::PackedStringArray(v) => {
                write!(f, "PackedStringArray[")?;
                for (i, s) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{s}\"")?;
                }
                write!(f, "]")
            }
            Self::PackedVector2Array(v) => write!(f, "PackedVector2Array(size={})", v.len()),
            Self::PackedVector3Array(v) => write!(f, "PackedVector3Array(size={})", v.len()),
            Self::PackedColorArray(v) => write!(f, "PackedColorArray(size={})", v.len()),
            Self::PackedVector4Array(v) => write!(f, "PackedVector4Array(size={})", v.len()),
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_i64(buf: &mut Vec<u8>, v: i64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_f32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_f64(buf: &mut Vec<u8>, v: f64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_u32(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
    // Pad to 4-byte alignment
    let pad = (4 - (bytes.len() % 4)) % 4;
    for _ in 0..pad {
        buf.push(0);
    }
}

fn write_f32_array(buf: &mut Vec<u8>, vals: &[f64]) {
    for &v in vals {
        write_f32(buf, v as f32);
    }
}

// ---------------------------------------------------------------------------
// Decoding helpers
// ---------------------------------------------------------------------------

fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() {
        return None;
    }
    let v = u32::from_le_bytes(data[*offset..*offset + 4].try_into().ok()?);
    *offset += 4;
    Some(v)
}

fn read_i32(data: &[u8], offset: &mut usize) -> Option<i32> {
    if *offset + 4 > data.len() {
        return None;
    }
    let v = i32::from_le_bytes(data[*offset..*offset + 4].try_into().ok()?);
    *offset += 4;
    Some(v)
}

fn read_i64(data: &[u8], offset: &mut usize) -> Option<i64> {
    if *offset + 8 > data.len() {
        return None;
    }
    let v = i64::from_le_bytes(data[*offset..*offset + 8].try_into().ok()?);
    *offset += 8;
    Some(v)
}

fn read_u64(data: &[u8], offset: &mut usize) -> Option<u64> {
    if *offset + 8 > data.len() {
        return None;
    }
    let v = u64::from_le_bytes(data[*offset..*offset + 8].try_into().ok()?);
    *offset += 8;
    Some(v)
}

fn read_f32(data: &[u8], offset: &mut usize) -> Option<f32> {
    if *offset + 4 > data.len() {
        return None;
    }
    let v = f32::from_le_bytes(data[*offset..*offset + 4].try_into().ok()?);
    *offset += 4;
    Some(v)
}

fn read_f64(data: &[u8], offset: &mut usize) -> Option<f64> {
    if *offset + 8 > data.len() {
        return None;
    }
    let v = f64::from_le_bytes(data[*offset..*offset + 8].try_into().ok()?);
    *offset += 8;
    Some(v)
}

fn read_string(data: &[u8], offset: &mut usize) -> Option<String> {
    let len = read_u32(data, offset)? as usize;
    if *offset + len > data.len() {
        return None;
    }
    let s = std::str::from_utf8(&data[*offset..*offset + len]).ok()?;
    let result = s.to_string();
    *offset += len;
    // Skip padding to 4-byte alignment
    let pad = (4 - (len % 4)) % 4;
    *offset += pad;
    Some(result)
}

fn read_f32_array(data: &[u8], offset: &mut usize, count: usize) -> Option<Vec<f64>> {
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        result.push(f64::from(read_f32(data, offset)?));
    }
    Some(result)
}

fn read_f64_array(data: &[u8], offset: &mut usize, count: usize) -> Option<Vec<f64>> {
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        result.push(read_f64(data, offset)?);
    }
    Some(result)
}

// ---------------------------------------------------------------------------
// Encode
// ---------------------------------------------------------------------------

/// Encode a `GodotVariant` into the Godot binary variant format (little-endian).
pub fn encode_variant(variant: &GodotVariant, buf: &mut Vec<u8>) {
    match variant {
        GodotVariant::Nil => {
            write_u32(buf, TYPE_NIL);
        }
        GodotVariant::Bool(v) => {
            write_u32(buf, TYPE_BOOL);
            write_u32(buf, u32::from(*v));
        }
        GodotVariant::Int(v) => {
            let val = *v;
            if val >= i64::from(i32::MIN) && val <= i64::from(i32::MAX) {
                write_u32(buf, TYPE_INT);
                write_i32(buf, val as i32);
            } else {
                write_u32(buf, TYPE_INT | ENCODE_FLAG_64);
                write_i64(buf, val);
            }
        }
        GodotVariant::Float(v) => {
            let val = *v;
            // Encode as f32 if it fits without loss, otherwise f64
            #[allow(clippy::float_cmp)]
            if (val as f32 as f64) == val {
                write_u32(buf, TYPE_FLOAT);
                write_f32(buf, val as f32);
            } else {
                write_u32(buf, TYPE_FLOAT | ENCODE_FLAG_64);
                write_f64(buf, val);
            }
        }
        GodotVariant::String(v) => {
            write_u32(buf, TYPE_STRING);
            write_string(buf, v);
        }
        GodotVariant::Vector2(x, y) => {
            write_u32(buf, TYPE_VECTOR2);
            write_f32_array(buf, &[*x, *y]);
        }
        GodotVariant::Vector2i(x, y) => {
            write_u32(buf, TYPE_VECTOR2I);
            write_i32(buf, *x);
            write_i32(buf, *y);
        }
        GodotVariant::Rect2(x, y, w, h) => {
            write_u32(buf, TYPE_RECT2);
            write_f32_array(buf, &[*x, *y, *w, *h]);
        }
        GodotVariant::Rect2i(x, y, w, h) => {
            write_u32(buf, TYPE_RECT2I);
            write_i32(buf, *x);
            write_i32(buf, *y);
            write_i32(buf, *w);
            write_i32(buf, *h);
        }
        GodotVariant::Vector3(x, y, z) => {
            write_u32(buf, TYPE_VECTOR3);
            write_f32_array(buf, &[*x, *y, *z]);
        }
        GodotVariant::Vector3i(x, y, z) => {
            write_u32(buf, TYPE_VECTOR3I);
            write_i32(buf, *x);
            write_i32(buf, *y);
            write_i32(buf, *z);
        }
        GodotVariant::Transform2D(v) => {
            write_u32(buf, TYPE_TRANSFORM2D);
            write_f32_array(buf, v);
        }
        GodotVariant::Vector4(x, y, z, w) => {
            write_u32(buf, TYPE_VECTOR4);
            write_f32_array(buf, &[*x, *y, *z, *w]);
        }
        GodotVariant::Vector4i(x, y, z, w) => {
            write_u32(buf, TYPE_VECTOR4I);
            write_i32(buf, *x);
            write_i32(buf, *y);
            write_i32(buf, *z);
            write_i32(buf, *w);
        }
        GodotVariant::Plane(a, b, c, d) => {
            write_u32(buf, TYPE_PLANE);
            write_f32_array(buf, &[*a, *b, *c, *d]);
        }
        GodotVariant::Quaternion(x, y, z, w) => {
            write_u32(buf, TYPE_QUATERNION);
            write_f32_array(buf, &[*x, *y, *z, *w]);
        }
        GodotVariant::Aabb(v) => {
            write_u32(buf, TYPE_AABB);
            write_f32_array(buf, v);
        }
        GodotVariant::Basis(v) => {
            write_u32(buf, TYPE_BASIS);
            write_f32_array(buf, v);
        }
        GodotVariant::Transform3D(v) => {
            write_u32(buf, TYPE_TRANSFORM3D);
            write_f32_array(buf, v);
        }
        GodotVariant::Projection(v) => {
            write_u32(buf, TYPE_PROJECTION);
            write_f32_array(buf, v);
        }
        GodotVariant::Color(r, g, b, a) => {
            write_u32(buf, TYPE_COLOR);
            write_f32(buf, *r);
            write_f32(buf, *g);
            write_f32(buf, *b);
            write_f32(buf, *a);
        }
        GodotVariant::StringName(v) => {
            write_u32(buf, TYPE_STRING_NAME);
            write_string(buf, v);
        }
        GodotVariant::NodePath(v) => {
            // Encode NodePath as a simple string-style path
            // Format: new-style flag (name_count with bit 31 clear, but we use the
            // simpler total-length encoding for plain string paths)
            // Actually, Godot encodes NodePath specially. For simplicity and
            // compatibility with the DAP protocol, encode as the sub-name form:
            // We parse the path into names and subnames.
            encode_node_path(buf, v);
        }
        GodotVariant::Rid(v) => {
            write_u32(buf, TYPE_RID);
            write_u64(buf, *v);
        }
        GodotVariant::Object { class, properties } => {
            write_u32(buf, TYPE_OBJECT);
            // Encode class name length (as raw string, not variant-encoded)
            let class_bytes = class.as_bytes();
            write_u32(buf, class_bytes.len() as u32);
            buf.extend_from_slice(class_bytes);
            let pad = (4 - (class_bytes.len() % 4)) % 4;
            for _ in 0..pad {
                buf.push(0);
            }
            write_u32(buf, properties.len() as u32);
            for (name, value) in properties {
                write_string(buf, name);
                encode_variant(value, buf);
            }
        }
        GodotVariant::ObjectId(id) => {
            write_u32(buf, TYPE_OBJECT | ENCODE_FLAG_OBJECT_AS_ID);
            write_u64(buf, *id);
        }
        GodotVariant::Callable => {
            // Callable has no wire representation
            write_u32(buf, TYPE_CALLABLE);
        }
        GodotVariant::Signal { name, object_id } => {
            write_u32(buf, TYPE_SIGNAL);
            write_string(buf, name);
            write_u64(buf, *object_id);
        }
        GodotVariant::Dictionary(entries) => {
            write_u32(buf, TYPE_DICTIONARY);
            write_u32(buf, entries.len() as u32);
            for (key, value) in entries {
                encode_variant(key, buf);
                encode_variant(value, buf);
            }
        }
        GodotVariant::Array(items) => {
            write_u32(buf, TYPE_ARRAY);
            write_u32(buf, items.len() as u32);
            for item in items {
                encode_variant(item, buf);
            }
        }
        GodotVariant::PackedByteArray(v) => {
            write_u32(buf, TYPE_PACKED_BYTE_ARRAY);
            write_u32(buf, v.len() as u32);
            buf.extend_from_slice(v);
            let pad = (4 - (v.len() % 4)) % 4;
            for _ in 0..pad {
                buf.push(0);
            }
        }
        GodotVariant::PackedInt32Array(v) => {
            write_u32(buf, TYPE_PACKED_INT32_ARRAY);
            write_u32(buf, v.len() as u32);
            for &val in v {
                write_i32(buf, val);
            }
        }
        GodotVariant::PackedInt64Array(v) => {
            write_u32(buf, TYPE_PACKED_INT64_ARRAY);
            write_u32(buf, v.len() as u32);
            for &val in v {
                write_i64(buf, val);
            }
        }
        GodotVariant::PackedFloat32Array(v) => {
            write_u32(buf, TYPE_PACKED_FLOAT32_ARRAY);
            write_u32(buf, v.len() as u32);
            for &val in v {
                write_f32(buf, val);
            }
        }
        GodotVariant::PackedFloat64Array(v) => {
            write_u32(buf, TYPE_PACKED_FLOAT64_ARRAY);
            write_u32(buf, v.len() as u32);
            for &val in v {
                write_f64(buf, val);
            }
        }
        GodotVariant::PackedStringArray(v) => {
            write_u32(buf, TYPE_PACKED_STRING_ARRAY);
            write_u32(buf, v.len() as u32);
            for s in v {
                write_string(buf, s);
            }
        }
        GodotVariant::PackedVector2Array(v) => {
            write_u32(buf, TYPE_PACKED_VECTOR2_ARRAY);
            write_u32(buf, v.len() as u32);
            for &(x, y) in v {
                write_f32(buf, x);
                write_f32(buf, y);
            }
        }
        GodotVariant::PackedVector3Array(v) => {
            write_u32(buf, TYPE_PACKED_VECTOR3_ARRAY);
            write_u32(buf, v.len() as u32);
            for &(x, y, z) in v {
                write_f32(buf, x);
                write_f32(buf, y);
                write_f32(buf, z);
            }
        }
        GodotVariant::PackedColorArray(v) => {
            write_u32(buf, TYPE_PACKED_COLOR_ARRAY);
            write_u32(buf, v.len() as u32);
            for &(r, g, b, a) in v {
                write_f32(buf, r);
                write_f32(buf, g);
                write_f32(buf, b);
                write_f32(buf, a);
            }
        }
        GodotVariant::PackedVector4Array(v) => {
            write_u32(buf, TYPE_PACKED_VECTOR4_ARRAY);
            write_u32(buf, v.len() as u32);
            for &(x, y, z, w) in v {
                write_f32(buf, x);
                write_f32(buf, y);
                write_f32(buf, z);
                write_f32(buf, w);
            }
        }
    }
}

fn encode_node_path(buf: &mut Vec<u8>, path: &str) {
    write_u32(buf, TYPE_NODE_PATH);

    if path.is_empty() {
        // Empty path: 3 u32 headers (new format)
        write_u32(buf, 0x8000_0000); // name_count=0, new format flag
        write_u32(buf, 0); // subname_count=0
        write_u32(buf, 0); // flags=0
        return;
    }

    let absolute = path.starts_with('/');
    let path_str = if absolute { &path[1..] } else { path };

    // Split into path and subnames (separated by ':')
    let (path_part, subname_part) = match path_str.split_once(':') {
        Some((p, s)) => (p, s),
        None => (path_str, ""),
    };

    let names: Vec<&str> = if path_part.is_empty() {
        Vec::new()
    } else {
        path_part.split('/').collect()
    };

    let subnames: Vec<&str> = if subname_part.is_empty() {
        Vec::new()
    } else {
        subname_part.split(':').collect()
    };

    // New format: 3 u32 headers [name_count|0x80000000, subname_count, flags]
    write_u32(buf, names.len() as u32 | 0x8000_0000);
    write_u32(buf, subnames.len() as u32);
    let flags: u32 = if absolute { 1 } else { 0 };
    write_u32(buf, flags);

    for name in &names {
        write_string(buf, name);
    }
    for subname in &subnames {
        write_string(buf, subname);
    }
}

// ---------------------------------------------------------------------------
// Decode
// ---------------------------------------------------------------------------

/// Decode a `GodotVariant` from the Godot binary variant format.
///
/// `offset` is advanced past the decoded data. Returns `None` on malformed input.
pub fn decode_variant(data: &[u8], offset: &mut usize) -> Option<GodotVariant> {
    let header = read_u32(data, offset)?;
    let type_id = header & TYPE_MASK;
    let flag_64 = (header & ENCODE_FLAG_64) != 0;

    match type_id {
        TYPE_NIL => Some(GodotVariant::Nil),

        TYPE_BOOL => {
            let v = read_u32(data, offset)?;
            Some(GodotVariant::Bool(v != 0))
        }

        TYPE_INT => {
            if flag_64 {
                Some(GodotVariant::Int(read_i64(data, offset)?))
            } else {
                Some(GodotVariant::Int(i64::from(read_i32(data, offset)?)))
            }
        }

        TYPE_FLOAT => {
            if flag_64 {
                Some(GodotVariant::Float(read_f64(data, offset)?))
            } else {
                Some(GodotVariant::Float(f64::from(read_f32(data, offset)?)))
            }
        }

        TYPE_STRING => {
            let s = read_string(data, offset)?;
            Some(GodotVariant::String(s))
        }

        TYPE_VECTOR2 => {
            let vals = read_floats(data, offset, 2, flag_64)?;
            Some(GodotVariant::Vector2(vals[0], vals[1]))
        }

        TYPE_VECTOR2I => {
            let x = read_i32(data, offset)?;
            let y = read_i32(data, offset)?;
            Some(GodotVariant::Vector2i(x, y))
        }

        TYPE_RECT2 => {
            let vals = read_floats(data, offset, 4, flag_64)?;
            Some(GodotVariant::Rect2(vals[0], vals[1], vals[2], vals[3]))
        }

        TYPE_RECT2I => {
            let x = read_i32(data, offset)?;
            let y = read_i32(data, offset)?;
            let w = read_i32(data, offset)?;
            let h = read_i32(data, offset)?;
            Some(GodotVariant::Rect2i(x, y, w, h))
        }

        TYPE_VECTOR3 => {
            let vals = read_floats(data, offset, 3, flag_64)?;
            Some(GodotVariant::Vector3(vals[0], vals[1], vals[2]))
        }

        TYPE_VECTOR3I => {
            let x = read_i32(data, offset)?;
            let y = read_i32(data, offset)?;
            let z = read_i32(data, offset)?;
            Some(GodotVariant::Vector3i(x, y, z))
        }

        TYPE_TRANSFORM2D => {
            let vals = read_floats(data, offset, 6, flag_64)?;
            let mut arr = [0.0; 6];
            arr.copy_from_slice(&vals);
            Some(GodotVariant::Transform2D(arr))
        }

        TYPE_VECTOR4 => {
            let vals = read_floats(data, offset, 4, flag_64)?;
            Some(GodotVariant::Vector4(vals[0], vals[1], vals[2], vals[3]))
        }

        TYPE_VECTOR4I => {
            let x = read_i32(data, offset)?;
            let y = read_i32(data, offset)?;
            let z = read_i32(data, offset)?;
            let w = read_i32(data, offset)?;
            Some(GodotVariant::Vector4i(x, y, z, w))
        }

        TYPE_PLANE => {
            let vals = read_floats(data, offset, 4, flag_64)?;
            Some(GodotVariant::Plane(vals[0], vals[1], vals[2], vals[3]))
        }

        TYPE_QUATERNION => {
            let vals = read_floats(data, offset, 4, flag_64)?;
            Some(GodotVariant::Quaternion(vals[0], vals[1], vals[2], vals[3]))
        }

        TYPE_AABB => {
            let vals = read_floats(data, offset, 6, flag_64)?;
            let mut arr = [0.0; 6];
            arr.copy_from_slice(&vals);
            Some(GodotVariant::Aabb(arr))
        }

        TYPE_BASIS => {
            let vals = read_floats(data, offset, 9, flag_64)?;
            let mut arr = [0.0; 9];
            arr.copy_from_slice(&vals);
            Some(GodotVariant::Basis(arr))
        }

        TYPE_TRANSFORM3D => {
            let vals = read_floats(data, offset, 12, flag_64)?;
            let mut arr = [0.0; 12];
            arr.copy_from_slice(&vals);
            Some(GodotVariant::Transform3D(arr))
        }

        TYPE_PROJECTION => {
            let vals = read_floats(data, offset, 16, flag_64)?;
            let mut arr = [0.0; 16];
            arr.copy_from_slice(&vals);
            Some(GodotVariant::Projection(arr))
        }

        TYPE_COLOR => {
            // Color is always f32
            let r = read_f32(data, offset)?;
            let g = read_f32(data, offset)?;
            let b = read_f32(data, offset)?;
            let a = read_f32(data, offset)?;
            Some(GodotVariant::Color(r, g, b, a))
        }

        TYPE_STRING_NAME => {
            let s = read_string(data, offset)?;
            Some(GodotVariant::StringName(s))
        }

        TYPE_NODE_PATH => decode_node_path(data, offset),

        TYPE_RID => {
            let v = read_u64(data, offset)?;
            Some(GodotVariant::Rid(v))
        }

        TYPE_OBJECT => {
            if flag_64 {
                // ENCODE_FLAG_OBJECT_AS_ID
                let id = read_u64(data, offset)?;
                Some(GodotVariant::ObjectId(id))
            } else {
                // Full object encoding
                let class = read_string(data, offset)?;
                let prop_count = read_u32(data, offset)? as usize;
                let mut properties = Vec::with_capacity(prop_count);
                for _ in 0..prop_count {
                    let name = read_string(data, offset)?;
                    let value = decode_variant(data, offset)?;
                    properties.push((name, value));
                }
                Some(GodotVariant::Object { class, properties })
            }
        }

        TYPE_CALLABLE => {
            // Skip — no meaningful wire representation
            Some(GodotVariant::Callable)
        }

        TYPE_SIGNAL => {
            let name = read_string(data, offset)?;
            let object_id = read_u64(data, offset)?;
            Some(GodotVariant::Signal { name, object_id })
        }

        TYPE_DICTIONARY => {
            // Godot 4.x typed dictionaries: bits 16-17 = key type kind, 18-19 = value type kind
            let key_kind = (header >> 16) & 0b11;
            let val_kind = (header >> 18) & 0b11;
            skip_container_type(data, offset, key_kind)?;
            skip_container_type(data, offset, val_kind)?;

            let count = read_u32(data, offset)? as usize;
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let key = decode_variant(data, offset)?;
                let value = decode_variant(data, offset)?;
                entries.push((key, value));
            }
            Some(GodotVariant::Dictionary(entries))
        }

        TYPE_ARRAY => {
            // Godot 4.x typed arrays: bits 16-17 encode the container type kind
            let type_kind = (header >> 16) & 0b11;
            skip_container_type(data, offset, type_kind)?;

            let count = read_u32(data, offset)? as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(decode_variant(data, offset)?);
            }
            Some(GodotVariant::Array(items))
        }

        TYPE_PACKED_BYTE_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            if *offset + count > data.len() {
                return None;
            }
            let v = data[*offset..*offset + count].to_vec();
            *offset += count;
            let pad = (4 - (count % 4)) % 4;
            *offset += pad;
            Some(GodotVariant::PackedByteArray(v))
        }

        TYPE_PACKED_INT32_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                v.push(read_i32(data, offset)?);
            }
            Some(GodotVariant::PackedInt32Array(v))
        }

        TYPE_PACKED_INT64_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                v.push(read_i64(data, offset)?);
            }
            Some(GodotVariant::PackedInt64Array(v))
        }

        TYPE_PACKED_FLOAT32_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                v.push(read_f32(data, offset)?);
            }
            Some(GodotVariant::PackedFloat32Array(v))
        }

        TYPE_PACKED_FLOAT64_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                v.push(read_f64(data, offset)?);
            }
            Some(GodotVariant::PackedFloat64Array(v))
        }

        TYPE_PACKED_STRING_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                v.push(read_string(data, offset)?);
            }
            Some(GodotVariant::PackedStringArray(v))
        }

        TYPE_PACKED_VECTOR2_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                let x = read_f32(data, offset)?;
                let y = read_f32(data, offset)?;
                v.push((x, y));
            }
            Some(GodotVariant::PackedVector2Array(v))
        }

        TYPE_PACKED_VECTOR3_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                let x = read_f32(data, offset)?;
                let y = read_f32(data, offset)?;
                let z = read_f32(data, offset)?;
                v.push((x, y, z));
            }
            Some(GodotVariant::PackedVector3Array(v))
        }

        TYPE_PACKED_COLOR_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                let r = read_f32(data, offset)?;
                let g = read_f32(data, offset)?;
                let b = read_f32(data, offset)?;
                let a = read_f32(data, offset)?;
                v.push((r, g, b, a));
            }
            Some(GodotVariant::PackedColorArray(v))
        }

        TYPE_PACKED_VECTOR4_ARRAY => {
            let count = read_u32(data, offset)? as usize;
            let mut v = Vec::with_capacity(count);
            for _ in 0..count {
                let x = read_f32(data, offset)?;
                let y = read_f32(data, offset)?;
                let z = read_f32(data, offset)?;
                let w = read_f32(data, offset)?;
                v.push((x, y, z, w));
            }
            Some(GodotVariant::PackedVector4Array(v))
        }

        _ => None, // Unknown type
    }
}

fn read_floats(data: &[u8], offset: &mut usize, count: usize, flag_64: bool) -> Option<Vec<f64>> {
    if flag_64 {
        read_f64_array(data, offset, count)
    } else {
        read_f32_array(data, offset, count)
    }
}

/// Skip over Godot 4.x container type metadata for typed Arrays and Dictionaries.
/// `type_kind` is 2 bits from the header: 0=none, 1=builtin(u32), 2=class_name(string), 3=script(string).
fn skip_container_type(data: &[u8], offset: &mut usize, type_kind: u32) -> Option<()> {
    match type_kind {
        0 => Some(()), // NONE — no extra data
        1 => {
            read_u32(data, offset)?;
            Some(())
        } // BUILTIN — 4-byte Variant::Type
        2 | 3 => {
            read_string(data, offset)?;
            Some(())
        } // CLASS_NAME or SCRIPT — string
        _ => None,
    }
}

fn decode_node_path(data: &[u8], offset: &mut usize) -> Option<GodotVariant> {
    let name_count_raw = read_u32(data, offset)?;

    // Check for new format (Godot 4.x always sets bit 31)
    if (name_count_raw & 0x8000_0000) != 0 {
        // New format: 3 u32 headers [name_count|0x80000000, subname_count, flags]
        let name_count = (name_count_raw & 0x7FFF_FFFF) as usize;
        let subname_count = read_u32(data, offset)? as usize;
        let flags = read_u32(data, offset)?;
        let absolute = (flags & 1) != 0;

        // Empty path
        if name_count == 0 && subname_count == 0 && !absolute {
            return Some(GodotVariant::NodePath(String::new()));
        }

        let mut names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            names.push(read_string(data, offset)?);
        }

        let mut subnames = Vec::with_capacity(subname_count);
        for _ in 0..subname_count {
            subnames.push(read_string(data, offset)?);
        }

        let mut path = String::new();
        if absolute {
            path.push('/');
        }
        path.push_str(&names.join("/"));
        if !subnames.is_empty() {
            path.push(':');
            path.push_str(&subnames.join(":"));
        }

        Some(GodotVariant::NodePath(path))
    } else {
        // Old format: name_count_raw is the byte length of a plain string path
        let len = name_count_raw as usize;
        if *offset + len > data.len() {
            return None;
        }
        let s = std::str::from_utf8(&data[*offset..*offset + len]).ok()?;
        let result = s.to_string();
        *offset += len;
        let pad = (4 - (len % 4)) % 4;
        *offset += pad;
        Some(GodotVariant::NodePath(result))
    }
}

// ---------------------------------------------------------------------------
// Packet framing
// ---------------------------------------------------------------------------

/// Encode a list of variants as a framed packet:
/// `[4 bytes: payload_size (uint32 LE)] [payload: Variant-encoded Array]`
pub fn encode_packet(variants: &[GodotVariant]) -> Vec<u8> {
    let array = GodotVariant::Array(variants.to_vec());
    let mut payload = Vec::new();
    encode_variant(&array, &mut payload);

    let mut packet = Vec::with_capacity(4 + payload.len());
    write_u32(&mut packet, payload.len() as u32);
    packet.extend_from_slice(&payload);
    packet
}

/// Decode a framed packet into a list of variants.
/// Expects: `[4 bytes: payload_size] [payload: Variant-encoded Array]`
pub fn decode_packet(data: &[u8]) -> Option<Vec<GodotVariant>> {
    if data.len() < 4 {
        return None;
    }
    let mut offset = 0;
    let payload_size = read_u32(data, &mut offset)? as usize;
    if offset + payload_size > data.len() {
        return None;
    }
    let variant = decode_variant(data, &mut offset)?;
    match variant {
        GodotVariant::Array(items) => Some(items),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode then decode, assert round-trip equality.
    fn round_trip(variant: &GodotVariant) -> GodotVariant {
        let mut buf = Vec::new();
        encode_variant(variant, &mut buf);
        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).expect("decode failed");
        assert_eq!(offset, buf.len(), "not all bytes consumed");
        decoded
    }

    #[test]
    fn nil() {
        assert_eq!(round_trip(&GodotVariant::Nil), GodotVariant::Nil);
    }

    #[test]
    fn bool_true() {
        let v = GodotVariant::Bool(true);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn bool_false() {
        let v = GodotVariant::Bool(false);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn int_small() {
        let v = GodotVariant::Int(42);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn int_negative() {
        let v = GodotVariant::Int(-100);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn int_i32_max() {
        let v = GodotVariant::Int(i64::from(i32::MAX));
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn int_i32_min() {
        let v = GodotVariant::Int(i64::from(i32::MIN));
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn int_large_needs_64() {
        let v = GodotVariant::Int(i64::from(i32::MAX) + 1);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn int_large_negative_needs_64() {
        let v = GodotVariant::Int(i64::from(i32::MIN) - 1);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn float_simple() {
        // 1.0 can be represented exactly as f32
        let v = GodotVariant::Float(1.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn float_needs_f64() {
        // This value cannot be exactly represented as f32
        let v = GodotVariant::Float(1.0000000000001);
        let decoded = round_trip(&v);
        match decoded {
            GodotVariant::Float(f) => {
                assert!((f - 1.0000000000001).abs() < 1e-15);
            }
            other => panic!("expected Float, got {other:?}"),
        }
    }

    #[test]
    fn float_zero() {
        let v = GodotVariant::Float(0.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn string_empty() {
        let v = GodotVariant::String(String::new());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn string_short() {
        let v = GodotVariant::String("hello".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn string_needs_padding() {
        // Length 5 needs 3 bytes padding to align to 4
        let v = GodotVariant::String("abcde".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn string_exact_alignment() {
        // Length 4: no padding needed
        let v = GodotVariant::String("abcd".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn string_unicode() {
        let v = GodotVariant::String("hello 🌍".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn vector2() {
        let v = GodotVariant::Vector2(1.0, 2.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn vector2i() {
        let v = GodotVariant::Vector2i(10, -20);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn rect2() {
        let v = GodotVariant::Rect2(1.0, 2.0, 3.0, 4.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn rect2i() {
        let v = GodotVariant::Rect2i(1, 2, 100, 200);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn vector3() {
        let v = GodotVariant::Vector3(1.0, 2.0, 3.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn vector3i() {
        let v = GodotVariant::Vector3i(1, 2, 3);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn transform2d() {
        let v = GodotVariant::Transform2D([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn vector4() {
        let v = GodotVariant::Vector4(1.0, 2.0, 3.0, 4.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn vector4i() {
        let v = GodotVariant::Vector4i(1, 2, 3, 4);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn plane() {
        let v = GodotVariant::Plane(0.0, 1.0, 0.0, 5.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn quaternion() {
        let v = GodotVariant::Quaternion(0.0, 0.0, 0.0, 1.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn aabb() {
        let v = GodotVariant::Aabb([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn basis() {
        let v = GodotVariant::Basis([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn transform3d() {
        let v = GodotVariant::Transform3D([
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 10.0, 20.0, 30.0,
        ]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn projection() {
        let v = GodotVariant::Projection([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn color() {
        let v = GodotVariant::Color(1.0, 0.5, 0.25, 1.0);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn string_name() {
        let v = GodotVariant::StringName("my_signal".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn node_path_empty() {
        let v = GodotVariant::NodePath(String::new());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn node_path_relative() {
        let v = GodotVariant::NodePath("Player/Sprite".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn node_path_absolute() {
        let v = GodotVariant::NodePath("/root/Main/Player".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn node_path_with_subname() {
        let v = GodotVariant::NodePath("Player:position".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn node_path_absolute_with_subnames() {
        let v = GodotVariant::NodePath("/root/Player:position:x".into());
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn rid() {
        let v = GodotVariant::Rid(12345);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn object_id() {
        let v = GodotVariant::ObjectId(9999);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn object_full() {
        let v = GodotVariant::Object {
            class: "Node2D".into(),
            properties: vec![
                ("name".into(), GodotVariant::String("Player".into())),
                ("health".into(), GodotVariant::Int(100)),
            ],
        };
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn callable() {
        let v = GodotVariant::Callable;
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn signal() {
        let v = GodotVariant::Signal {
            name: "pressed".into(),
            object_id: 12345,
        };
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn dictionary_empty() {
        let v = GodotVariant::Dictionary(vec![]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn dictionary_with_entries() {
        let v = GodotVariant::Dictionary(vec![
            (GodotVariant::String("key".into()), GodotVariant::Int(42)),
            (
                GodotVariant::String("name".into()),
                GodotVariant::String("test".into()),
            ),
        ]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn array_empty() {
        let v = GodotVariant::Array(vec![]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn array_mixed() {
        let v = GodotVariant::Array(vec![
            GodotVariant::Int(1),
            GodotVariant::String("two".into()),
            GodotVariant::Bool(true),
            GodotVariant::Nil,
        ]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn array_nested() {
        let v = GodotVariant::Array(vec![
            GodotVariant::Array(vec![GodotVariant::Int(1), GodotVariant::Int(2)]),
            GodotVariant::Dictionary(vec![(
                GodotVariant::String("k".into()),
                GodotVariant::Float(3.0),
            )]),
        ]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_byte_array() {
        let v = GodotVariant::PackedByteArray(vec![1, 2, 3, 4, 5]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_byte_array_empty() {
        let v = GodotVariant::PackedByteArray(vec![]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_int32_array() {
        let v = GodotVariant::PackedInt32Array(vec![1, -2, 3, i32::MAX, i32::MIN]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_int64_array() {
        let v = GodotVariant::PackedInt64Array(vec![1, -2, i64::MAX, i64::MIN]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_float32_array() {
        let v = GodotVariant::PackedFloat32Array(vec![1.0, -2.5, 0.0]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_float64_array() {
        let v = GodotVariant::PackedFloat64Array(vec![1.0, -2.5, std::f64::consts::PI]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_string_array() {
        let v = GodotVariant::PackedStringArray(vec!["hello".into(), "world".into(), "".into()]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_vector2_array() {
        let v = GodotVariant::PackedVector2Array(vec![(1.0, 2.0), (3.0, 4.0)]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_vector3_array() {
        let v = GodotVariant::PackedVector3Array(vec![(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_color_array() {
        let v = GodotVariant::PackedColorArray(vec![(1.0, 0.0, 0.0, 1.0), (0.0, 1.0, 0.0, 0.5)]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packed_vector4_array() {
        let v = GodotVariant::PackedVector4Array(vec![(1.0, 2.0, 3.0, 4.0)]);
        assert_eq!(round_trip(&v), v);
    }

    #[test]
    fn packet_round_trip() {
        let variants = vec![
            GodotVariant::String("hello".into()),
            GodotVariant::Int(42),
            GodotVariant::Bool(true),
        ];
        let packet = encode_packet(&variants);
        let decoded = decode_packet(&packet).expect("packet decode failed");
        assert_eq!(decoded, variants);
    }

    #[test]
    fn packet_empty() {
        let variants: Vec<GodotVariant> = vec![];
        let packet = encode_packet(&variants);
        let decoded = decode_packet(&packet).expect("packet decode failed");
        assert_eq!(decoded, variants);
    }

    #[test]
    fn decode_truncated_returns_none() {
        // Just the header, no data
        let buf = [0x02, 0x00, 0x00, 0x00]; // TYPE_INT, no data following
        let mut offset = 0;
        assert!(decode_variant(&buf, &mut offset).is_none());
    }

    #[test]
    fn decode_empty_returns_none() {
        let buf = [];
        let mut offset = 0;
        assert!(decode_variant(&buf, &mut offset).is_none());
    }

    #[test]
    fn decode_unknown_type_returns_none() {
        let buf = [0xFF, 0x00, 0x00, 0x00]; // type 255
        let mut offset = 0;
        assert!(decode_variant(&buf, &mut offset).is_none());
    }

    #[test]
    fn display_nil() {
        assert_eq!(GodotVariant::Nil.to_string(), "null");
    }

    #[test]
    fn display_vector3() {
        assert_eq!(
            GodotVariant::Vector3(1.0, 2.0, 3.0).to_string(),
            "Vector3(1, 2, 3)"
        );
    }

    #[test]
    fn display_array() {
        let v = GodotVariant::Array(vec![GodotVariant::Int(1), GodotVariant::Int(2)]);
        assert_eq!(v.to_string(), "[1, 2]");
    }

    #[test]
    fn display_dictionary() {
        let v = GodotVariant::Dictionary(vec![(
            GodotVariant::String("key".into()),
            GodotVariant::Int(42),
        )]);
        assert_eq!(v.to_string(), "{\"key\": 42}");
    }

    #[test]
    fn serde_json_nil() {
        let v = GodotVariant::Nil;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"type":"Nil"}"#);
    }

    #[test]
    fn serde_json_int() {
        let v = GodotVariant::Int(42);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, r#"{"type":"Int","value":42}"#);
    }

    #[test]
    fn serde_json_array() {
        let v = GodotVariant::Array(vec![GodotVariant::Int(1), GodotVariant::Bool(true)]);
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"type\":\"Array\""));
    }

    #[test]
    fn int_encoding_size() {
        // Small int should be 4+4=8 bytes (header + i32)
        let mut buf = Vec::new();
        encode_variant(&GodotVariant::Int(42), &mut buf);
        assert_eq!(buf.len(), 8);

        // Large int should be 4+8=12 bytes (header with FLAG_64 + i64)
        buf.clear();
        encode_variant(&GodotVariant::Int(i64::from(i32::MAX) + 1), &mut buf);
        assert_eq!(buf.len(), 12);
    }

    #[test]
    fn float_encoding_size() {
        // Simple float should be 4+4=8 bytes (header + f32)
        let mut buf = Vec::new();
        encode_variant(&GodotVariant::Float(1.0), &mut buf);
        assert_eq!(buf.len(), 8);
    }

    #[test]
    fn string_padding_bytes() {
        // "hi" = 2 bytes, needs 2 bytes padding
        // Total: 4 (header) + 4 (length) + 2 (data) + 2 (pad) = 12
        let mut buf = Vec::new();
        encode_variant(&GodotVariant::String("hi".into()), &mut buf);
        assert_eq!(buf.len(), 12);
    }

    #[test]
    fn decode_f64_vector2() {
        // Manually encode a Vector2 with FLAG_64 to test f64 decoding
        let mut buf = Vec::new();
        write_u32(&mut buf, TYPE_VECTOR2 | ENCODE_FLAG_64);
        write_f64(&mut buf, 1.5);
        write_f64(&mut buf, 2.5);

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        assert_eq!(decoded, GodotVariant::Vector2(1.5, 2.5));
    }

    #[test]
    fn decode_f64_basis() {
        // Manually encode a Basis with FLAG_64
        let mut buf = Vec::new();
        write_u32(&mut buf, TYPE_BASIS | ENCODE_FLAG_64);
        let vals = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        for v in vals {
            write_f64(&mut buf, v);
        }

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        assert_eq!(decoded, GodotVariant::Basis(vals));
    }

    #[test]
    fn node_path_encoding_size() {
        // NodePath uses 3 u32 headers (Godot 4.x new format):
        // type_header(4) + name_count(4) + subname_count(4) + flags(4) = 16 for empty
        let mut buf = Vec::new();
        encode_variant(&GodotVariant::NodePath(String::new()), &mut buf);
        assert_eq!(buf.len(), 16);
    }

    #[test]
    fn decode_godot_encoded_node_path() {
        // Simulate how Godot 4.x encodes NodePath("/root/Player:position")
        // 3 u32 headers: name_count|0x80000000, subname_count, flags
        let mut buf = Vec::new();
        write_u32(&mut buf, TYPE_NODE_PATH);
        write_u32(&mut buf, 2 | 0x8000_0000); // 2 names, new format
        write_u32(&mut buf, 1); // 1 subname
        write_u32(&mut buf, 1); // flags: absolute=true
        write_string(&mut buf, "root");
        write_string(&mut buf, "Player");
        write_string(&mut buf, "position");

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        assert_eq!(
            decoded,
            GodotVariant::NodePath("/root/Player:position".into())
        );
        assert_eq!(offset, buf.len());
    }

    #[test]
    fn decode_godot_node_path_in_array() {
        // NodePath embedded in an Array (like inspect_objects response)
        let mut buf = Vec::new();
        // Array with 3 elements: Int, String, NodePath
        write_u32(&mut buf, TYPE_ARRAY);
        write_u32(&mut buf, 3);
        // Element 0: Int(42)
        write_u32(&mut buf, TYPE_INT);
        write_i32(&mut buf, 42);
        // Element 1: String("test")
        write_u32(&mut buf, TYPE_STRING);
        write_string(&mut buf, "test");
        // Element 2: NodePath("Player/Sprite")
        write_u32(&mut buf, TYPE_NODE_PATH);
        write_u32(&mut buf, 2 | 0x8000_0000); // 2 names, new format
        write_u32(&mut buf, 0); // 0 subnames
        write_u32(&mut buf, 0); // flags: not absolute
        write_string(&mut buf, "Player");
        write_string(&mut buf, "Sprite");

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        match decoded {
            GodotVariant::Array(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], GodotVariant::Int(42));
                assert_eq!(items[1], GodotVariant::String("test".into()));
                assert_eq!(items[2], GodotVariant::NodePath("Player/Sprite".into()));
            }
            other => panic!("expected Array, got {other:?}"),
        }
        assert_eq!(offset, buf.len());
    }

    #[test]
    fn multiple_variants_sequential() {
        // Encode multiple variants into one buffer, decode sequentially
        let mut buf = Vec::new();
        encode_variant(&GodotVariant::Int(1), &mut buf);
        encode_variant(&GodotVariant::String("test".into()), &mut buf);
        encode_variant(&GodotVariant::Bool(false), &mut buf);

        let mut offset = 0;
        assert_eq!(
            decode_variant(&buf, &mut offset).unwrap(),
            GodotVariant::Int(1)
        );
        assert_eq!(
            decode_variant(&buf, &mut offset).unwrap(),
            GodotVariant::String("test".into())
        );
        assert_eq!(
            decode_variant(&buf, &mut offset).unwrap(),
            GodotVariant::Bool(false)
        );
        assert_eq!(offset, buf.len());
    }

    #[test]
    fn decode_typed_array_builtin() {
        // Godot 4.x typed Array[int]: header has BUILTIN kind (0b01) in bits 16-17,
        // followed by 4-byte element type, then count + elements.
        let mut buf = Vec::new();
        // Header: TYPE_ARRAY | (BUILTIN << 16) = 28 | (1 << 16) = 0x1001C
        write_u32(&mut buf, TYPE_ARRAY | (1 << 16));
        // Container type: Variant::Type = TYPE_INT (2)
        write_u32(&mut buf, TYPE_INT);
        // Count: 2 elements
        write_u32(&mut buf, 2);
        // Element 0: Int(10)
        write_u32(&mut buf, TYPE_INT);
        write_i32(&mut buf, 10);
        // Element 1: Int(20)
        write_u32(&mut buf, TYPE_INT);
        write_i32(&mut buf, 20);

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        assert_eq!(
            decoded,
            GodotVariant::Array(vec![GodotVariant::Int(10), GodotVariant::Int(20)])
        );
        assert_eq!(offset, buf.len());
    }

    #[test]
    fn decode_typed_array_class_name() {
        // Godot 4.x typed Array[Node]: header has CLASS_NAME kind (0b10) in bits 16-17,
        // followed by class name string, then count + elements.
        let mut buf = Vec::new();
        // Header: TYPE_ARRAY | (CLASS_NAME << 16) = 28 | (2 << 16)
        write_u32(&mut buf, TYPE_ARRAY | (2 << 16));
        // Container type: class name string
        write_string(&mut buf, "Node");
        // Count: 0 elements (empty typed array)
        write_u32(&mut buf, 0);

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        assert_eq!(decoded, GodotVariant::Array(vec![]));
        assert_eq!(offset, buf.len());
    }

    #[test]
    fn decode_typed_dictionary() {
        // Godot 4.x typed Dictionary[String, int]:
        // bits 16-17 = key kind (BUILTIN=0b01), bits 18-19 = value kind (BUILTIN=0b01)
        let mut buf = Vec::new();
        // Header: TYPE_DICTIONARY | (BUILTIN << 16) | (BUILTIN << 18)
        write_u32(&mut buf, TYPE_DICTIONARY | (1 << 16) | (1 << 18));
        // Key container type: TYPE_STRING (4)
        write_u32(&mut buf, TYPE_STRING);
        // Value container type: TYPE_INT (2)
        write_u32(&mut buf, TYPE_INT);
        // Count: 1 entry
        write_u32(&mut buf, 1);
        // Key: String("hello")
        write_u32(&mut buf, TYPE_STRING);
        write_string(&mut buf, "hello");
        // Value: Int(42)
        write_u32(&mut buf, TYPE_INT);
        write_i32(&mut buf, 42);

        let mut offset = 0;
        let decoded = decode_variant(&buf, &mut offset).unwrap();
        assert_eq!(
            decoded,
            GodotVariant::Dictionary(vec![(
                GodotVariant::String("hello".into()),
                GodotVariant::Int(42)
            )])
        );
        assert_eq!(offset, buf.len());
    }

    #[test]
    fn decode_typed_array_in_packet() {
        // Simulate a Godot response containing a typed array property
        // This tests that typed arrays don't break packet-level decoding
        let inner = vec![
            GodotVariant::String("test_cmd".into()),
            GodotVariant::Int(1),
            // The third element would be a typed array in a real response
        ];
        let packet = encode_packet(&inner);
        let decoded = decode_packet(&packet).unwrap();
        assert_eq!(decoded[0], GodotVariant::String("test_cmd".into()));
    }
}
