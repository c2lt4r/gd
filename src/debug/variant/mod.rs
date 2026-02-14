#![allow(dead_code)]

#[cfg(test)]
mod tests;

pub mod decode;
pub mod encode;

pub use decode::{decode_packet, decode_variant};
pub use encode::encode_packet;

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
    #[allow(clippy::too_many_lines)]
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
