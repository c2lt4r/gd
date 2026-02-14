use super::{
    ENCODE_FLAG_64, ENCODE_FLAG_OBJECT_AS_ID, GodotVariant, TYPE_AABB, TYPE_ARRAY, TYPE_BASIS,
    TYPE_BOOL, TYPE_CALLABLE, TYPE_COLOR, TYPE_DICTIONARY, TYPE_FLOAT, TYPE_INT, TYPE_NIL,
    TYPE_NODE_PATH, TYPE_OBJECT, TYPE_PACKED_BYTE_ARRAY, TYPE_PACKED_COLOR_ARRAY,
    TYPE_PACKED_FLOAT32_ARRAY, TYPE_PACKED_FLOAT64_ARRAY, TYPE_PACKED_INT32_ARRAY,
    TYPE_PACKED_INT64_ARRAY, TYPE_PACKED_STRING_ARRAY, TYPE_PACKED_VECTOR2_ARRAY,
    TYPE_PACKED_VECTOR3_ARRAY, TYPE_PACKED_VECTOR4_ARRAY, TYPE_PLANE, TYPE_PROJECTION,
    TYPE_QUATERNION, TYPE_RECT2, TYPE_RECT2I, TYPE_RID, TYPE_SIGNAL, TYPE_STRING, TYPE_STRING_NAME,
    TYPE_TRANSFORM2D, TYPE_TRANSFORM3D, TYPE_VECTOR2, TYPE_VECTOR2I, TYPE_VECTOR3, TYPE_VECTOR3I,
    TYPE_VECTOR4, TYPE_VECTOR4I,
};

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

pub(crate) fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn write_i32(buf: &mut Vec<u8>, v: i32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_i64(buf: &mut Vec<u8>, v: i64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn write_f32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn write_f64(buf: &mut Vec<u8>, v: f64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn write_string(buf: &mut Vec<u8>, s: &str) {
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
// Encode
// ---------------------------------------------------------------------------

/// Encode a `GodotVariant` into the Godot binary variant format (little-endian).
#[allow(clippy::too_many_lines)]
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
            if i32::try_from(val).is_ok() {
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
            // compatibility with the binary debug protocol, encode as the sub-name form:
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
    let flags: u32 = u32::from(absolute);
    write_u32(buf, flags);

    for name in &names {
        write_string(buf, name);
    }
    for subname in &subnames {
        write_string(buf, subname);
    }
}

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
