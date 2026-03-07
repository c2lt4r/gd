use std::collections::HashMap;
use std::fmt;

/// A runtime GDScript object instance.
#[derive(Debug, Clone)]
pub struct GdObject {
    pub class_name: String,
    pub properties: HashMap<String, GdValue>,
}

#[derive(Debug, Clone)]
pub enum GdValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    GdString(String),
    StringName(String),
    Array(Vec<GdValue>),
    Dictionary(Vec<(GdValue, GdValue)>),
    Vector2(f64, f64),
    Vector2i(i64, i64),
    Vector3(f64, f64, f64),
    Vector3i(i64, i64, i64),
    Vector4(f64, f64, f64, f64),
    Color(f64, f64, f64, f64),
    Rect2(f64, f64, f64, f64),
    NodePath(String),
    Callable { name: String },
    Object(Box<GdObject>),
}

impl PartialEq for GdValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::GdString(a), Self::GdString(b))
            | (Self::StringName(a), Self::StringName(b))
            | (Self::NodePath(a), Self::NodePath(b))
            | (Self::Callable { name: a }, Self::Callable { name: b }) => a == b,
            // Object identity comparison (same allocation = same object)
            (Self::Object(a), Self::Object(b)) => std::ptr::eq(a.as_ref(), b.as_ref()),
            (Self::Array(a), Self::Array(b)) => a == b,
            (Self::Dictionary(a), Self::Dictionary(b)) => a == b,
            (Self::Vector2(x1, y1), Self::Vector2(x2, y2)) => {
                x1.to_bits() == x2.to_bits() && y1.to_bits() == y2.to_bits()
            }
            (Self::Vector2i(x1, y1), Self::Vector2i(x2, y2)) => x1 == x2 && y1 == y2,
            (Self::Vector3(x1, y1, z1), Self::Vector3(x2, y2, z2)) => {
                x1.to_bits() == x2.to_bits()
                    && y1.to_bits() == y2.to_bits()
                    && z1.to_bits() == z2.to_bits()
            }
            (Self::Vector3i(x1, y1, z1), Self::Vector3i(x2, y2, z2)) => {
                x1 == x2 && y1 == y2 && z1 == z2
            }
            (Self::Vector4(x1, y1, z1, w1), Self::Vector4(x2, y2, z2, w2))
            | (Self::Color(x1, y1, z1, w1), Self::Color(x2, y2, z2, w2))
            | (Self::Rect2(x1, y1, z1, w1), Self::Rect2(x2, y2, z2, w2)) => {
                x1.to_bits() == x2.to_bits()
                    && y1.to_bits() == y2.to_bits()
                    && z1.to_bits() == z2.to_bits()
                    && w1.to_bits() == w2.to_bits()
            }
            _ => false,
        }
    }
}

impl fmt::Display for GdValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("null"),
            Self::Bool(b) => f.write_str(if *b { "true" } else { "false" }),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    write!(f, "{v:.1}")
                } else {
                    write!(f, "{v}")
                }
            }
            Self::GdString(s) => f.write_str(s),
            Self::StringName(s) => write!(f, "&{s}"),
            Self::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Self::Dictionary(entries) => {
                f.write_str("{ ")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                f.write_str(" }")
            }
            Self::Vector2(x, y) => write!(f, "({x}, {y})"),
            Self::Vector2i(x, y) => write!(f, "({x}, {y})"),
            Self::Vector3(x, y, z) => write!(f, "({x}, {y}, {z})"),
            Self::Vector3i(x, y, z) => write!(f, "({x}, {y}, {z})"),
            Self::Vector4(x, y, z, w) => write!(f, "({x}, {y}, {z}, {w})"),
            Self::Color(r, g, b, a) => write!(f, "({r}, {g}, {b}, {a})"),
            Self::Rect2(x, y, w, h) => write!(f, "[P: ({x}, {y}), S: ({w}, {h})]"),
            Self::NodePath(p) => write!(f, "NodePath({p})"),
            Self::Callable { name } => write!(f, "Callable({name})"),
            Self::Object(obj) => write!(f, "<{}>", obj.class_name),
        }
    }
}

impl GdValue {
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Int(n) => *n != 0,
            Self::Float(v) => *v != 0.0,
            Self::GdString(s) | Self::StringName(s) => !s.is_empty(),
            Self::Array(items) => !items.is_empty(),
            Self::Dictionary(entries) => !entries.is_empty(),
            _ => true,
        }
    }

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::GdString(_) => "String",
            Self::StringName(_) => "StringName",
            Self::Array(_) => "Array",
            Self::Dictionary(_) => "Dictionary",
            Self::Vector2(..) => "Vector2",
            Self::Vector2i(..) => "Vector2i",
            Self::Vector3(..) => "Vector3",
            Self::Vector3i(..) => "Vector3i",
            Self::Vector4(..) => "Vector4",
            Self::Color(..) => "Color",
            Self::Rect2(..) => "Rect2",
            Self::NodePath(_) => "NodePath",
            Self::Callable { .. } => "Callable",
            Self::Object(_) => "Object",
        }
    }

    /// Returns the class name for objects, or the built-in type name otherwise.
    #[must_use]
    pub fn class_name(&self) -> &str {
        match self {
            Self::Object(obj) => &obj.class_name,
            _ => self.type_name(),
        }
    }

    #[must_use]
    pub fn type_id(&self) -> i64 {
        match self {
            Self::Null => 0,
            Self::Bool(_) => 1,
            Self::Int(_) => 2,
            Self::Float(_) => 3,
            Self::GdString(_) => 4,
            Self::Vector2(..) => 5,
            Self::Vector2i(..) => 6,
            Self::Rect2(..) => 7,
            Self::Vector3(..) => 9,
            Self::Vector3i(..) => 10,
            Self::Vector4(..) => 12,
            Self::Color(..) => 20,
            Self::StringName(_) => 21,
            Self::NodePath(_) => 22,
            Self::Object(_) => 24,
            Self::Callable { .. } => 25,
            Self::Dictionary(_) => 27,
            Self::Array(_) => 28,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_null() {
        assert_eq!(GdValue::Null.to_string(), "null");
    }

    #[test]
    fn display_bool() {
        assert_eq!(GdValue::Bool(true).to_string(), "true");
        assert_eq!(GdValue::Bool(false).to_string(), "false");
    }

    #[test]
    fn display_int() {
        assert_eq!(GdValue::Int(42).to_string(), "42");
        assert_eq!(GdValue::Int(-1).to_string(), "-1");
    }

    #[test]
    fn display_float() {
        assert_eq!(GdValue::Float(3.125).to_string(), "3.125");
        assert_eq!(GdValue::Float(1.0).to_string(), "1.0");
        assert_eq!(GdValue::Float(0.0).to_string(), "0.0");
    }

    #[test]
    fn display_string() {
        assert_eq!(GdValue::GdString("hello".into()).to_string(), "hello");
    }

    #[test]
    fn display_string_name() {
        assert_eq!(GdValue::StringName("ready".into()).to_string(), "&ready");
    }

    #[test]
    fn display_array() {
        let arr = GdValue::Array(vec![GdValue::Int(1), GdValue::Int(2), GdValue::Int(3)]);
        assert_eq!(arr.to_string(), "[1, 2, 3]");
        assert_eq!(GdValue::Array(vec![]).to_string(), "[]");
    }

    #[test]
    fn display_dictionary() {
        let dict = GdValue::Dictionary(vec![(GdValue::GdString("key".into()), GdValue::Int(10))]);
        assert_eq!(dict.to_string(), "{ key: 10 }");
    }

    #[test]
    fn display_vector2() {
        assert_eq!(GdValue::Vector2(1.0, 2.0).to_string(), "(1, 2)");
    }

    #[test]
    fn display_vector2i() {
        assert_eq!(GdValue::Vector2i(3, 4).to_string(), "(3, 4)");
    }

    #[test]
    fn display_vector3() {
        assert_eq!(GdValue::Vector3(1.0, 2.0, 3.0).to_string(), "(1, 2, 3)");
    }

    #[test]
    fn display_vector3i() {
        assert_eq!(GdValue::Vector3i(1, 2, 3).to_string(), "(1, 2, 3)");
    }

    #[test]
    fn display_vector4() {
        assert_eq!(
            GdValue::Vector4(1.0, 2.0, 3.0, 4.0).to_string(),
            "(1, 2, 3, 4)"
        );
    }

    #[test]
    fn display_color() {
        assert_eq!(
            GdValue::Color(1.0, 0.0, 0.0, 1.0).to_string(),
            "(1, 0, 0, 1)"
        );
    }

    #[test]
    fn display_rect2() {
        assert_eq!(
            GdValue::Rect2(0.0, 0.0, 100.0, 200.0).to_string(),
            "[P: (0, 0), S: (100, 200)]"
        );
    }

    #[test]
    fn display_node_path() {
        assert_eq!(
            GdValue::NodePath("Player/Sprite".into()).to_string(),
            "NodePath(Player/Sprite)"
        );
    }

    #[test]
    fn display_callable() {
        assert_eq!(
            GdValue::Callable {
                name: "my_func".into()
            }
            .to_string(),
            "Callable(my_func)"
        );
    }

    #[test]
    fn truthy_null_is_false() {
        assert!(!GdValue::Null.is_truthy());
    }

    #[test]
    fn truthy_bool() {
        assert!(GdValue::Bool(true).is_truthy());
        assert!(!GdValue::Bool(false).is_truthy());
    }

    #[test]
    fn truthy_int() {
        assert!(GdValue::Int(1).is_truthy());
        assert!(!GdValue::Int(0).is_truthy());
    }

    #[test]
    fn truthy_float() {
        assert!(GdValue::Float(0.1).is_truthy());
        assert!(!GdValue::Float(0.0).is_truthy());
    }

    #[test]
    fn truthy_string() {
        assert!(GdValue::GdString("hi".into()).is_truthy());
        assert!(!GdValue::GdString(String::new()).is_truthy());
    }

    #[test]
    fn truthy_array() {
        assert!(GdValue::Array(vec![GdValue::Null]).is_truthy());
        assert!(!GdValue::Array(vec![]).is_truthy());
    }

    #[test]
    fn truthy_dictionary() {
        let dict = GdValue::Dictionary(vec![(GdValue::Int(1), GdValue::Int(2))]);
        assert!(dict.is_truthy());
        assert!(!GdValue::Dictionary(vec![]).is_truthy());
    }

    #[test]
    fn truthy_vectors_always_true() {
        assert!(GdValue::Vector2(0.0, 0.0).is_truthy());
        assert!(GdValue::Vector3i(0, 0, 0).is_truthy());
        assert!(GdValue::Color(0.0, 0.0, 0.0, 0.0).is_truthy());
    }

    #[test]
    fn type_name_correctness() {
        assert_eq!(GdValue::Null.type_name(), "null");
        assert_eq!(GdValue::Bool(true).type_name(), "bool");
        assert_eq!(GdValue::Int(0).type_name(), "int");
        assert_eq!(GdValue::Float(0.0).type_name(), "float");
        assert_eq!(GdValue::GdString(String::new()).type_name(), "String");
        assert_eq!(GdValue::StringName(String::new()).type_name(), "StringName");
        assert_eq!(GdValue::Array(vec![]).type_name(), "Array");
        assert_eq!(GdValue::Dictionary(vec![]).type_name(), "Dictionary");
        assert_eq!(GdValue::Vector2(0.0, 0.0).type_name(), "Vector2");
        assert_eq!(GdValue::Vector2i(0, 0).type_name(), "Vector2i");
        assert_eq!(GdValue::Vector3(0.0, 0.0, 0.0).type_name(), "Vector3");
        assert_eq!(GdValue::Vector3i(0, 0, 0).type_name(), "Vector3i");
        assert_eq!(GdValue::Vector4(0.0, 0.0, 0.0, 0.0).type_name(), "Vector4");
        assert_eq!(GdValue::Color(0.0, 0.0, 0.0, 0.0).type_name(), "Color");
        assert_eq!(GdValue::Rect2(0.0, 0.0, 0.0, 0.0).type_name(), "Rect2");
        assert_eq!(GdValue::NodePath(String::new()).type_name(), "NodePath");
        assert_eq!(
            GdValue::Callable {
                name: String::new()
            }
            .type_name(),
            "Callable"
        );
    }

    #[test]
    fn type_id_values() {
        assert_eq!(GdValue::Null.type_id(), 0);
        assert_eq!(GdValue::Bool(true).type_id(), 1);
        assert_eq!(GdValue::Int(0).type_id(), 2);
        assert_eq!(GdValue::Float(0.0).type_id(), 3);
        assert_eq!(GdValue::GdString(String::new()).type_id(), 4);
        assert_eq!(GdValue::Vector2(0.0, 0.0).type_id(), 5);
        assert_eq!(GdValue::Vector2i(0, 0).type_id(), 6);
        assert_eq!(GdValue::Rect2(0.0, 0.0, 0.0, 0.0).type_id(), 7);
        assert_eq!(GdValue::Vector3(0.0, 0.0, 0.0).type_id(), 9);
        assert_eq!(GdValue::Vector3i(0, 0, 0).type_id(), 10);
        assert_eq!(GdValue::Vector4(0.0, 0.0, 0.0, 0.0).type_id(), 12);
        assert_eq!(GdValue::Color(0.0, 0.0, 0.0, 0.0).type_id(), 20);
        assert_eq!(GdValue::StringName(String::new()).type_id(), 21);
        assert_eq!(GdValue::NodePath(String::new()).type_id(), 22);
        assert_eq!(
            GdValue::Callable {
                name: String::new()
            }
            .type_id(),
            25
        );
        assert_eq!(GdValue::Dictionary(vec![]).type_id(), 27);
        assert_eq!(GdValue::Array(vec![]).type_id(), 28);
    }

    #[test]
    fn partial_eq_float_bitwise() {
        assert_eq!(GdValue::Float(1.0), GdValue::Float(1.0));
        // Bitwise equality: same NaN bit pattern is equal
        assert_eq!(GdValue::Float(f64::NAN), GdValue::Float(f64::NAN));
        assert_ne!(GdValue::Float(1.0), GdValue::Float(2.0));
    }

    #[test]
    fn partial_eq_different_variants() {
        assert_ne!(GdValue::Int(1), GdValue::Float(1.0));
        assert_ne!(GdValue::Null, GdValue::Bool(false));
    }
}
