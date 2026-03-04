use super::{
    ENCODE_FLAG_64, GodotVariant, TYPE_AABB, TYPE_ARRAY, TYPE_BASIS, TYPE_BOOL, TYPE_CALLABLE,
    TYPE_COLOR, TYPE_DICTIONARY, TYPE_FLOAT, TYPE_INT, TYPE_MASK, TYPE_NIL, TYPE_NODE_PATH,
    TYPE_OBJECT, TYPE_PACKED_BYTE_ARRAY, TYPE_PACKED_COLOR_ARRAY, TYPE_PACKED_FLOAT32_ARRAY,
    TYPE_PACKED_FLOAT64_ARRAY, TYPE_PACKED_INT32_ARRAY, TYPE_PACKED_INT64_ARRAY,
    TYPE_PACKED_STRING_ARRAY, TYPE_PACKED_VECTOR2_ARRAY, TYPE_PACKED_VECTOR3_ARRAY,
    TYPE_PACKED_VECTOR4_ARRAY, TYPE_PLANE, TYPE_PROJECTION, TYPE_QUATERNION, TYPE_RECT2,
    TYPE_RECT2I, TYPE_RID, TYPE_SIGNAL, TYPE_STRING, TYPE_STRING_NAME, TYPE_TRANSFORM2D,
    TYPE_TRANSFORM3D, TYPE_VECTOR2, TYPE_VECTOR2I, TYPE_VECTOR3, TYPE_VECTOR3I, TYPE_VECTOR4,
    TYPE_VECTOR4I,
};

// ---------------------------------------------------------------------------
// Decoding helpers
// ---------------------------------------------------------------------------

pub(crate) fn read_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() {
        return None;
    }
    let v = u32::from_le_bytes(data[*offset..*offset + 4].try_into().ok()?);
    *offset += 4;
    Some(v)
}

pub(crate) fn read_i32(data: &[u8], offset: &mut usize) -> Option<i32> {
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

pub(crate) fn read_f32(data: &[u8], offset: &mut usize) -> Option<f32> {
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

pub(crate) fn read_string(data: &[u8], offset: &mut usize) -> Option<String> {
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

fn read_floats(data: &[u8], offset: &mut usize, count: usize, flag_64: bool) -> Option<Vec<f64>> {
    if flag_64 {
        read_f64_array(data, offset, count)
    } else {
        read_f32_array(data, offset, count)
    }
}

// ---------------------------------------------------------------------------
// Decode
// ---------------------------------------------------------------------------

/// Decode a `GodotVariant` from the Godot binary variant format.
///
/// `offset` is advanced past the decoded data. Returns `None` on malformed input.
#[allow(clippy::too_many_lines)]
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
            // Skip -- no meaningful wire representation
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

/// Skip over Godot 4.x container type metadata for typed Arrays and Dictionaries.
/// `type_kind` is 2 bits from the header: 0=none, 1=builtin(u32), 2=class_name(string), 3=script(string).
fn skip_container_type(data: &[u8], offset: &mut usize, type_kind: u32) -> Option<()> {
    match type_kind {
        0 => Some(()), // NONE -- no extra data
        1 => {
            read_u32(data, offset)?;
            Some(())
        } // BUILTIN -- 4-byte Variant::Type
        2 | 3 => {
            read_string(data, offset)?;
            Some(())
        } // CLASS_NAME or SCRIPT -- string
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
