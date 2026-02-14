use std::sync::Arc;

use super::DaemonServer;

/// Clone the debug server Arc from the daemon mutex.
/// This releases the mutex immediately so other daemon queries aren't blocked
/// while long-running debug commands (batch inspect, accept, etc.) execute.
pub(super) fn get_debug_server(
    server: &DaemonServer,
) -> Option<Arc<crate::debug::godot_debug_server::GodotDebugServer>> {
    server.debug_server.lock().unwrap().as_ref().map(Arc::clone)
}

pub(super) fn json_to_variant(value: &serde_json::Value) -> crate::debug::variant::GodotVariant {
    use crate::debug::variant::GodotVariant;
    match value {
        serde_json::Value::Null => GodotVariant::Nil,
        serde_json::Value::Bool(b) => GodotVariant::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                GodotVariant::Int(i)
            } else if let Some(f) = n.as_f64() {
                GodotVariant::Float(f)
            } else {
                GodotVariant::Nil
            }
        }
        serde_json::Value::String(s) => GodotVariant::String(s.clone()),
        serde_json::Value::Array(arr) => json_array_to_variant(arr),
        serde_json::Value::Object(obj) => json_object_to_variant(obj),
    }
}

/// Convert a JSON array to the best-fit GodotVariant based on element count.
/// Float arrays: 2->Vector2, 3->Vector3, 4->Vector4, 6->Transform2D, 9->Basis, 12->Transform3D, 16->Projection
/// Int arrays (all integers): 2->Vector2i, 3->Vector3i, 4->Vector4i
/// Mixed/other: generic Array with recursive conversion.
pub(super) fn json_array_to_variant(
    arr: &[serde_json::Value],
) -> crate::debug::variant::GodotVariant {
    use crate::debug::variant::GodotVariant;

    // Check if all elements are numbers
    let all_numbers = arr.iter().all(serde_json::Value::is_number);
    if !all_numbers {
        // Generic array — recurse into each element
        return GodotVariant::Array(arr.iter().map(json_to_variant).collect());
    }

    // Check if all elements are integers (no fractional part)
    let all_ints = arr.iter().all(|v| v.as_i64().is_some());

    let floats: Vec<f64> = arr.iter().filter_map(serde_json::Value::as_f64).collect();
    if floats.len() != arr.len() {
        return GodotVariant::Array(arr.iter().map(json_to_variant).collect());
    }

    match floats.len() {
        2 if all_ints => GodotVariant::Vector2i(floats[0] as i32, floats[1] as i32),
        2 => GodotVariant::Vector2(floats[0], floats[1]),
        3 if all_ints => {
            GodotVariant::Vector3i(floats[0] as i32, floats[1] as i32, floats[2] as i32)
        }
        3 => GodotVariant::Vector3(floats[0], floats[1], floats[2]),
        4 if all_ints => GodotVariant::Vector4i(
            floats[0] as i32,
            floats[1] as i32,
            floats[2] as i32,
            floats[3] as i32,
        ),
        4 => GodotVariant::Vector4(floats[0], floats[1], floats[2], floats[3]),
        6 => GodotVariant::Transform2D([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
        ]),
        9 => GodotVariant::Basis([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5], floats[6], floats[7],
            floats[8],
        ]),
        12 => GodotVariant::Transform3D([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5], floats[6], floats[7],
            floats[8], floats[9], floats[10], floats[11],
        ]),
        16 => GodotVariant::Projection([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5], floats[6], floats[7],
            floats[8], floats[9], floats[10], floats[11], floats[12], floats[13], floats[14],
            floats[15],
        ]),
        _ => GodotVariant::Array(arr.iter().map(json_to_variant).collect()),
    }
}

/// Convert a JSON object to a GodotVariant.
/// Supports typed wrappers: `{"Vector3": [1,2,3]}`, `{"Color": [1,0,0,1]}`, etc.
/// Falls back to Dictionary for unrecognized shapes.
pub(super) fn json_object_to_variant(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> crate::debug::variant::GodotVariant {
    use crate::debug::variant::GodotVariant;

    // Single-key type wrapper: {"Vector3": [1.0, 2.0, 3.0]}
    if obj.len() == 1 {
        let (key, inner) = obj.iter().next().unwrap();
        if let Some(arr) = inner.as_array() {
            let floats: Vec<f64> = arr.iter().filter_map(serde_json::Value::as_f64).collect();
            if floats.len() == arr.len() {
                match (key.as_str(), floats.len()) {
                    ("Vector2", 2) => return GodotVariant::Vector2(floats[0], floats[1]),
                    ("Vector2i", 2) => {
                        return GodotVariant::Vector2i(floats[0] as i32, floats[1] as i32);
                    }
                    ("Rect2", 4) => {
                        return GodotVariant::Rect2(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("Rect2i", 4) => {
                        return GodotVariant::Rect2i(
                            floats[0] as i32,
                            floats[1] as i32,
                            floats[2] as i32,
                            floats[3] as i32,
                        );
                    }
                    ("Vector3", 3) => {
                        return GodotVariant::Vector3(floats[0], floats[1], floats[2]);
                    }
                    ("Vector3i", 3) => {
                        return GodotVariant::Vector3i(
                            floats[0] as i32,
                            floats[1] as i32,
                            floats[2] as i32,
                        );
                    }
                    ("Transform2D", 6) => {
                        return GodotVariant::Transform2D([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                        ]);
                    }
                    ("Vector4", 4) => {
                        return GodotVariant::Vector4(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("Vector4i", 4) => {
                        return GodotVariant::Vector4i(
                            floats[0] as i32,
                            floats[1] as i32,
                            floats[2] as i32,
                            floats[3] as i32,
                        );
                    }
                    ("Plane", 4) => {
                        return GodotVariant::Plane(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("Quaternion", 4) => {
                        return GodotVariant::Quaternion(
                            floats[0], floats[1], floats[2], floats[3],
                        );
                    }
                    ("AABB", 6) => {
                        return GodotVariant::Aabb([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                        ]);
                    }
                    ("Basis", 9) => {
                        return GodotVariant::Basis([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                            floats[6], floats[7], floats[8],
                        ]);
                    }
                    ("Transform3D", 12) => {
                        return GodotVariant::Transform3D([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                            floats[6], floats[7], floats[8], floats[9], floats[10], floats[11],
                        ]);
                    }
                    ("Projection", 16) => {
                        return GodotVariant::Projection([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                            floats[6], floats[7], floats[8], floats[9], floats[10], floats[11],
                            floats[12], floats[13], floats[14], floats[15],
                        ]);
                    }
                    ("Color", 4) => {
                        return GodotVariant::Color(
                            floats[0] as f32,
                            floats[1] as f32,
                            floats[2] as f32,
                            floats[3] as f32,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    // Generic dictionary
    GodotVariant::Dictionary(
        obj.iter()
            .map(|(k, v)| (GodotVariant::String(k.clone()), json_to_variant(v)))
            .collect(),
    )
}

/// Set a named sub-field on a GodotVariant (client-side fieldwise assignment).
#[allow(clippy::too_many_lines)]
pub(super) fn variant_set_field(
    target: &mut crate::debug::variant::GodotVariant,
    field: &str,
    value: &crate::debug::variant::GodotVariant,
) -> bool {
    use crate::debug::variant::GodotVariant;

    let as_f64 = match value {
        GodotVariant::Float(f) => Some(*f),
        GodotVariant::Int(i) => Some(*i as f64),
        _ => None,
    };
    let as_f32 = as_f64.map(|f| f as f32);
    let as_i32 = match value {
        GodotVariant::Int(i) => Some(*i as i32),
        GodotVariant::Float(f) => Some(*f as i32),
        _ => None,
    };

    match target {
        GodotVariant::Vector2(x, y) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                _ => return false,
            }
        }
        GodotVariant::Vector2i(x, y) => {
            let Some(v) = as_i32 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                _ => return false,
            }
        }
        GodotVariant::Vector3(x, y, z) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                _ => return false,
            }
        }
        GodotVariant::Vector3i(x, y, z) => {
            let Some(v) = as_i32 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                _ => return false,
            }
        }
        GodotVariant::Vector4(x, y, z, w) | GodotVariant::Quaternion(x, y, z, w) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                "w" => *w = v,
                _ => return false,
            }
        }
        GodotVariant::Vector4i(x, y, z, w) => {
            let Some(v) = as_i32 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                "w" => *w = v,
                _ => return false,
            }
        }
        GodotVariant::Color(r, g, b, a) => {
            let Some(v) = as_f32 else { return false };
            match field {
                "r" => *r = v,
                "g" => *g = v,
                "b" => *b = v,
                "a" => *a = v,
                _ => return false,
            }
        }
        GodotVariant::Rect2(x, y, w, h) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "w" | "width" => *w = v,
                "h" | "height" => *h = v,
                _ => return false,
            }
        }
        GodotVariant::Plane(a, b, c, d) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *a = v,
                "y" => *b = v,
                "z" => *c = v,
                "d" => *d = v,
                _ => return false,
            }
        }
        _ => return false,
    }
    true
}
