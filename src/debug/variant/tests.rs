use crate::debug::variant::decode::decode_variant;
use crate::debug::variant::encode::{
    encode_packet, encode_variant, write_f64, write_i32, write_string, write_u32,
};
use crate::debug::variant::{
    ENCODE_FLAG_64, GodotVariant, TYPE_ARRAY, TYPE_BASIS, TYPE_DICTIONARY, TYPE_INT,
    TYPE_NODE_PATH, TYPE_STRING, TYPE_VECTOR2, decode_packet,
};

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
    let v = GodotVariant::Float(1.000_000_000_000_1);
    let decoded = round_trip(&v);
    match decoded {
        GodotVariant::Float(f) => {
            assert!((f - 1.000_000_000_000_1).abs() < 1e-15);
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
    let v = GodotVariant::String("hello \u{1f30d}".into());
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
    let v = GodotVariant::PackedStringArray(vec!["hello".into(), "world".into(), String::new()]);
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
