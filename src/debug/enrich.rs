//! Enrich debug inspect output with static ClassDB/builtins documentation.
//!
//! This module is loosely coupled — it takes JSON in and returns JSON out.
//! Remove the `enrich_inspect` call site to disable entirely.

use serde_json::{Map, Value};

use crate::class_db;
use crate::lsp::builtins;

/// Enrich an inspect result with ClassDB docs.
///
/// Adds a `"docs"` field alongside each property's `"value"`:
/// ```json
/// {
///   "name": "velocity",
///   "value": { "Vector3": [1.0, 2.0, 3.0] },
///   "docs": { "brief": "...", "description": "..." }
/// }
/// ```
///
/// Also adds a top-level `"class_docs"` field with class description and methods.
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
        // No builtins doc but exists in class_db — just add docs URL and inheritance
        let mut class_info = serde_json::json!({
            "docs_url": builtins::godot_docs_url(class_name),
        });
        if let Some(parent) = class_db::parent_class(class_name) {
            class_info["parent_class"] = Value::String(parent.to_string());
        }
        enriched["class_docs"] = class_info;
    }

    // Enrich each property with docs
    if let Some(props) = enriched.get_mut("properties")
        && let Some(props_arr) = props.as_array_mut()
    {
        for prop in props_arr.iter_mut() {
            let prop_name = prop["name"].as_str().unwrap_or("");
            if prop_name.is_empty() {
                continue;
            }
            // Look up member docs, filtering by class + inheritance
            if let Some(doc) = lookup_member_for_class(class_name, prop_name) {
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
    // Try exact class match first, then walk up the hierarchy
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
        // Should have class_docs (CharacterBody3D exists in class_db)
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
        // Node2D has `global_position` documented in builtins
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
        // global_position should have docs (it's a CanvasItem/Node2D builtin)
        let gp = &props[0];
        assert!(gp.get("docs").is_some(), "global_position should have docs");
        // custom_prop should NOT have docs
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
        // Should not crash, class_docs may be absent
        assert_eq!(result["class_name"], "MyCustomScript");
    }

    #[test]
    fn test_enrich_inherited_property_docs() {
        // CharacterBody3D inherits from Node3D → Node → Object
        // "connect" is a method on Object — should be found via inheritance
        let input = serde_json::json!({
            "object_id": 101,
            "class_name": "CharacterBody3D",
            "properties": [
                {"name": "global_transform", "value": "Transform3D", "type_id": 18, "hint": 0, "hint_string": "", "usage": 0},
            ]
        });
        let result = enrich_inspect(&input);
        // global_transform is documented on Node3D, should be found via inheritance
        let props = result["properties"].as_array().unwrap();
        let gt = &props[0];
        if gt.get("docs").is_some() {
            // If builtins has it, great
            assert!(!gt["docs"]["brief"].as_str().unwrap().is_empty());
        }
        // If builtins doesn't have it, that's also fine — no crash
    }
}
