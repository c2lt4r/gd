# Godot Variant Binary Encoding (marshalls.cpp)

Reference for `src/debug/variant.rs`. Source: `godot/core/io/marshalls.cpp` (Godot master).

## Header Layout (u32)

```
Bits 0-7:   Variant type (0-38)
Bits 8-15:  Unused
Bit  16:    ENCODE_FLAG_64 (for Int/Float) or OBJECT_AS_ID (for Object)
Bits 16-17: TYPED_ARRAY kind (for Array) or TYPED_DICTIONARY_KEY kind (for Dict)
Bits 18-19: TYPED_DICTIONARY_VALUE kind (for Dict)
```

```c
#define HEADER_TYPE_MASK 0xFF
#define HEADER_DATA_FLAG_64 (1 << 16)
#define HEADER_DATA_FLAG_OBJECT_AS_ID (1 << 16)
#define HEADER_DATA_FIELD_TYPED_ARRAY_MASK (0b11 << 16)
#define HEADER_DATA_FIELD_TYPED_ARRAY_SHIFT 16
#define HEADER_DATA_FIELD_TYPED_DICTIONARY_KEY_MASK (0b11 << 16)
#define HEADER_DATA_FIELD_TYPED_DICTIONARY_KEY_SHIFT 16
#define HEADER_DATA_FIELD_TYPED_DICTIONARY_VALUE_MASK (0b11 << 18)
#define HEADER_DATA_FIELD_TYPED_DICTIONARY_VALUE_SHIFT 18
```

## Container Type Kinds (for typed Array/Dictionary)

```c
enum ContainerTypeKind {
    CONTAINER_TYPE_KIND_NONE       = 0b00,  // No extra data
    CONTAINER_TYPE_KIND_BUILTIN    = 0b01,  // 4-byte u32 (Variant::Type)
    CONTAINER_TYPE_KIND_CLASS_NAME = 0b10,  // length-prefixed string (class name)
    CONTAINER_TYPE_KIND_SCRIPT     = 0b11,  // length-prefixed string (script path)
};
```

For Array: kind is in bits 16-17.
For Dictionary: key kind in bits 16-17, value kind in bits 18-19.

The container type data is written BEFORE the element count.

## Wire Format by Type

### Primitives
- **NIL (0)**: header only (0 bytes data)
- **BOOL (1)**: u32 (0 or 1)
- **INT (2)**: i32 (or i64 if FLAG_64)
- **FLOAT (3)**: f32 (or f64 if FLAG_64)
- **STRING (4)**: u32(len) + bytes + padding to 4-byte boundary

### Math Types (all f32, or f64 if FLAG_64)
- **VECTOR2 (5)**: 2 floats
- **VECTOR2I (6)**: 2 i32
- **RECT2 (7)**: 4 floats
- **RECT2I (8)**: 4 i32
- **VECTOR3 (9)**: 3 floats
- **VECTOR3I (10)**: 3 i32
- **TRANSFORM2D (11)**: 6 floats
- **VECTOR4 (12)**: 4 floats
- **VECTOR4I (13)**: 4 i32
- **PLANE (14)**: 4 floats
- **QUATERNION (15)**: 4 floats
- **AABB (16)**: 6 floats
- **BASIS (17)**: 9 floats
- **TRANSFORM3D (18)**: 12 floats
- **PROJECTION (19)**: 16 floats

### Other Types
- **COLOR (20)**: 4 f32 (always f32, no f64 flag)
- **STRING_NAME (21)**: same as STRING
- **NODE_PATH (22)**: special encoding (see below)
- **RID (23)**: u64
- **OBJECT (24)**: if OBJECT_AS_ID flag: u64; else: string(class) + u32(prop_count) + [string(name) + variant(value)]*
- **CALLABLE (25)**: EMPTY (0 bytes data)
- **SIGNAL (26)**: string(name) + u64(object_id)
- **DICTIONARY (27)**: [key_type_data] + [val_type_data] + u32(count) + [variant(key) + variant(value)]*
- **ARRAY (28)**: [element_type_data] + u32(count) + [variant(element)]*

### Packed Arrays
- **PACKED_BYTE_ARRAY (29)**: u32(count) + bytes + padding
- **PACKED_INT32_ARRAY (30)**: u32(count) + i32*
- **PACKED_INT64_ARRAY (31)**: u32(count) + i64*
- **PACKED_FLOAT32_ARRAY (32)**: u32(count) + f32*
- **PACKED_FLOAT64_ARRAY (33)**: u32(count) + f64*
- **PACKED_STRING_ARRAY (34)**: u32(count) + string*
- **PACKED_VECTOR2_ARRAY (35)**: u32(count) + (f32,f32)*
- **PACKED_VECTOR3_ARRAY (36)**: u32(count) + (f32,f32,f32)*
- **PACKED_COLOR_ARRAY (37)**: u32(count) + (f32,f32,f32,f32)*
- **PACKED_VECTOR4_ARRAY (38)**: u32(count) + (f32,f32,f32,f32)*

## NodePath Encoding

New format (Godot 4.x, bit 31 set in first u32):
```
u32: name_count | 0x80000000
u32: subname_count
u32: flags (bit 0 = absolute)
string[name_count]: path components
string[subname_count]: subname components
```

Old format (bit 31 clear): treated as string length.

## Packet Framing

```
u32: payload_size
payload: variant-encoded Array
```

The outer Array typically contains 3 elements for debug protocol:
```
[String(command), Int(thread_id), Array(data)]
```

## Key Functions in marshalls.cpp

- `_encode_string` / `_decode_string`: length-prefixed UTF-8 with 4-byte pad
- `_encode_container_type` / `_decode_container_type`: typed container metadata
- `encode_variant_traits` / `_decode_variant`: full variant encode/decode
