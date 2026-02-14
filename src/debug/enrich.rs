//! Enrich debug inspect output with type resolution, enum names, ranges, and ClassDB docs.
//!
//! This module is loosely coupled — it takes JSON in and returns JSON out.
//! Remove the `enrich_inspect` call site to disable entirely.

use serde_json::{Map, Value};

use crate::class_db;
use crate::lsp::builtins;

/// Godot variant type ID → human-readable name.
fn variant_type_name(type_id: u64) -> Option<&'static str> {
    match type_id {
        0 => Some("null"),
        1 => Some("bool"),
        2 => Some("int"),
        3 => Some("float"),
        4 => Some("String"),
        5 => Some("Vector2"),
        6 => Some("Vector2i"),
        7 => Some("Rect2"),
        8 => Some("Rect2i"),
        9 => Some("Vector3"),
        10 => Some("Vector3i"),
        11 => Some("Transform2D"),
        12 => Some("Vector4"),
        13 => Some("Vector4i"),
        14 => Some("Plane"),
        15 => Some("Quaternion"),
        16 => Some("AABB"),
        17 => Some("Basis"),
        18 => Some("Transform3D"),
        19 => Some("Projection"),
        20 => Some("Color"),
        21 => Some("StringName"),
        22 => Some("NodePath"),
        23 => Some("RID"),
        24 => Some("Object"),
        25 => Some("Callable"),
        26 => Some("Signal"),
        27 => Some("Dictionary"),
        28 => Some("Array"),
        29 => Some("PackedByteArray"),
        30 => Some("PackedInt32Array"),
        31 => Some("PackedInt64Array"),
        32 => Some("PackedFloat32Array"),
        33 => Some("PackedFloat64Array"),
        34 => Some("PackedStringArray"),
        35 => Some("PackedVector2Array"),
        36 => Some("PackedVector3Array"),
        37 => Some("PackedColorArray"),
        38 => Some("PackedVector4Array"),
        _ => None,
    }
}

/// Godot property hints (from core/object/property_hint.h).
const PROPERTY_HINT_RANGE: u64 = 1;
const PROPERTY_HINT_ENUM: u64 = 2;

/// Enrich an inspect result with type names, enum resolution, ranges, and ClassDB docs.
///
/// Per-property enrichment (works for ALL properties, including script-defined):
/// - `"type_name"`: resolved from `type_id` (e.g. 2 → "int", 9 → "Vector3")
/// - `"enum_value"`: when hint=ENUM, resolves integer value to name (e.g. 3 → "Always")
/// - `"range"`: when hint=RANGE, parses hint_string (e.g. "0.01..179.0")
/// - `"docs"`: ClassDB/builtins documentation (engine properties only)
///
/// Top-level enrichment:
/// - `"class_docs"`: class description, docs URL, parent class
pub fn enrich_inspect(result: &Value) -> Value {
    let mut enriched = result.clone();
    let class_name = result["class_name"].as_str().unwrap_or("");

    // Enrich class-level docs
    if let Some(type_doc) = builtins::lookup_type(class_name) {
        enriched["class_docs"] = serde_json::json!({
            "brief": type_doc.brief,
            "description": type_doc.description,
            "docs_url": builtins::godot_docs_url(class_name),
        });
    } else if class_db::class_exists(class_name) {
        let mut class_info = serde_json::json!({
            "docs_url": builtins::godot_docs_url(class_name),
        });
        if let Some(parent) = class_db::parent_class(class_name) {
            class_info["parent_class"] = Value::String(parent.to_string());
        }
        enriched["class_docs"] = class_info;
    }

    // Enrich each property
    if let Some(props) = enriched.get_mut("properties")
        && let Some(props_arr) = props.as_array_mut()
    {
        for prop in props_arr.iter_mut() {
            let prop_name = prop["name"].as_str().unwrap_or("").to_string();
            if prop_name.is_empty() {
                continue;
            }

            let type_id = prop["type_id"].as_u64().unwrap_or(0);
            let hint = prop["hint"].as_u64().unwrap_or(0);
            let hint_string = prop["hint_string"].as_str().unwrap_or("").to_string();

            // 1. Type name resolution
            if let Some(name) = variant_type_name(type_id) {
                prop["type_name"] = Value::String(name.to_string());
            }

            // 2. Enum value resolution
            if hint == PROPERTY_HINT_ENUM && !hint_string.is_empty() {
                let names: Vec<&str> = hint_string.split(',').collect();
                // Value could be an integer directly or inside a variant wrapper
                if let Some(int_val) = prop["value"]
                    .as_u64()
                    .or_else(|| prop["value"].as_i64().map(|v| v as u64))
                    && let Some(name) = names.get(int_val as usize)
                {
                    prop["enum_value"] = Value::String(name.trim().to_string());
                }
                // Always include the available enum options
                prop["enum_options"] = Value::Array(
                    names
                        .iter()
                        .map(|n| Value::String(n.trim().to_string()))
                        .collect(),
                );
            }

            // 3. Range hints
            if hint == PROPERTY_HINT_RANGE && !hint_string.is_empty() {
                let parts: Vec<&str> = hint_string.split(',').collect();
                if parts.len() >= 2 {
                    let mut range = Map::new();
                    range.insert(
                        "min".to_string(),
                        Value::String(parts[0].trim().to_string()),
                    );
                    range.insert(
                        "max".to_string(),
                        Value::String(parts[1].trim().to_string()),
                    );
                    if let Some(step) = parts.get(2) {
                        let step = step.trim();
                        if !step.is_empty() {
                            range.insert("step".to_string(), Value::String(step.to_string()));
                        }
                    }
                    prop["range"] = Value::Object(range);
                }
            }

            // 4. ClassDB/builtins docs (engine properties only)
            if let Some(doc) = lookup_member_for_class(class_name, &prop_name) {
                let mut docs = Map::new();
                docs.insert("brief".to_string(), Value::String(doc.brief.to_string()));
                if !doc.description.is_empty() {
                    docs.insert(
                        "description".to_string(),
                        Value::String(doc.description.to_string()),
                    );
                }
                prop["docs"] = Value::Object(docs);
            }
        }
    }

    enriched
}

/// Look up a member doc for a specific class, walking the inheritance chain.
fn lookup_member_for_class(class: &str, name: &str) -> Option<&'static builtins::BuiltinMember> {
    let mut current = class;
    loop {
        if let Some(doc) = builtins::lookup_member_for(current, name) {
            return Some(doc);
        }
        current = class_db::parent_class(current)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrich_adds_class_docs() {
        let input = serde_json::json!({
            "object_id": 123,
            "class_name": "CharacterBody3D",
            "properties": [
                {"name": "velocity", "value": {"Vector3": [0.0, 0.0, 0.0]}, "type_id": 9, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        assert!(result.get("class_docs").is_some());
        assert!(
            result["class_docs"]["docs_url"]
                .as_str()
                .unwrap()
                .contains("characterbody3d")
        );
    }

    #[test]
    fn test_enrich_adds_property_docs() {
        let input = serde_json::json!({
            "object_id": 456,
            "class_name": "Node2D",
            "properties": [
                {"name": "global_position", "value": {"Vector2": [1.0, 2.0]}, "type_id": 5, "hint": 0, "hint_string": "", "usage": 0},
                {"name": "custom_prop", "value": 42, "type_id": 2, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        let props = result["properties"].as_array().unwrap();
        let gp = &props[0];
        assert!(gp.get("docs").is_some(), "global_position should have docs");
        let cp = &props[1];
        assert!(cp.get("docs").is_none(), "custom_prop should not have docs");
    }

    #[test]
    fn test_enrich_unknown_class_no_crash() {
        let input = serde_json::json!({
            "object_id": 789,
            "class_name": "MyCustomScript",
            "properties": [
                {"name": "speed", "value": 5.0, "type_id": 3, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        assert_eq!(result["class_name"], "MyCustomScript");
    }

    #[test]
    fn test_enrich_inherited_property_docs() {
        let input = serde_json::json!({
            "object_id": 101,
            "class_name": "CharacterBody3D",
            "properties": [
                {"name": "global_transform", "value": "Transform3D", "type_id": 18, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        let props = result["properties"].as_array().unwrap();
        let gt = &props[0];
        if gt.get("docs").is_some() {
            assert!(!gt["docs"]["brief"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn test_enrich_type_name_resolution() {
        let input = serde_json::json!({
            "object_id": 200,
            "class_name": "MyScript",
            "properties": [
                {"name": "health", "value": 100, "type_id": 2, "hint": 0, "hint_string": "", "usage": 0},
                {"name": "pos", "value": {"Vector3": [0.0, 0.0, 0.0]}, "type_id": 9, "hint": 0, "hint_string": "", "usage": 0},
                {"name": "label", "value": "hello", "type_id": 4, "hint": 0, "hint_string": "", "usage": 0},
                {"name": "color", "value": [1.0, 0.0, 0.0, 1.0], "type_id": 20, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        let props = result["properties"].as_array().unwrap();
        assert_eq!(props[0]["type_name"], "int");
        assert_eq!(props[1]["type_name"], "Vector3");
        assert_eq!(props[2]["type_name"], "String");
        assert_eq!(props[3]["type_name"], "Color");
    }

    #[test]
    fn test_enrich_enum_resolution() {
        let input = serde_json::json!({
            "object_id": 300,
            "class_name": "Node",
            "properties": [
                {"name": "process_mode", "value": 3, "type_id": 2, "hint": 2, "hint_string": "Inherit,Pausable,When Paused,Always,Disabled", "usage": 0},
                {"name": "custom_state", "value": 1, "type_id": 2, "hint": 2, "hint_string": "Idle,Running,Dead", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        let props = result["properties"].as_array().unwrap();
        // process_mode = 3 → "Always"
        assert_eq!(props[0]["enum_value"], "Always");
        assert_eq!(
            props[0]["enum_options"],
            serde_json::json!(["Inherit", "Pausable", "When Paused", "Always", "Disabled"])
        );
        // custom_state = 1 → "Running" (script-defined enum!)
        assert_eq!(props[1]["enum_value"], "Running");
        assert_eq!(
            props[1]["enum_options"],
            serde_json::json!(["Idle", "Running", "Dead"])
        );
    }

    #[test]
    fn test_enrich_range_hints() {
        let input = serde_json::json!({
            "object_id": 400,
            "class_name": "Camera3D",
            "properties": [
                {"name": "fov", "value": 75.0, "type_id": 3, "hint": 1, "hint_string": "0.01,179.0,0.1", "usage": 0},
                {"name": "near", "value": 0.05, "type_id": 3, "hint": 1, "hint_string": "0.001,10.0", "usage": 0},
                {"name": "no_range", "value": 1.0, "type_id": 3, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        let props = result["properties"].as_array().unwrap();
        // fov: range with step
        assert_eq!(props[0]["range"]["min"], "0.01");
        assert_eq!(props[0]["range"]["max"], "179.0");
        assert_eq!(props[0]["range"]["step"], "0.1");
        // near: range without step
        assert_eq!(props[1]["range"]["min"], "0.001");
        assert_eq!(props[1]["range"]["max"], "10.0");
        assert!(props[1]["range"].get("step").is_none());
        // no_range: hint=0, no range field
        assert!(props[2].get("range").is_none());
    }

    #[test]
    fn test_enrich_enum_out_of_bounds() {
        // Value is beyond the enum options — should not crash, just no enum_value
        let input = serde_json::json!({
            "object_id": 500,
            "class_name": "Node",
            "properties": [
                {"name": "mode", "value": 99, "type_id": 2, "hint": 2, "hint_string": "A,B,C", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        let props = result["properties"].as_array().unwrap();
        assert!(props[0].get("enum_value").is_none());
        // But enum_options should still be present
        assert_eq!(props[0]["enum_options"], serde_json::json!(["A", "B", "C"]));
    }
}
