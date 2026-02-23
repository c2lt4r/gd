use crate::core::scene::{self, Connection, ExtResource, SceneData, SceneNode};

use super::attach_script::insert_script_attachment;
use super::*;

fn make_scene_data(ext_ids: &[&str], node_name: &str) -> SceneData {
    SceneData {
        ext_resources: ext_ids
            .iter()
            .map(|id| ExtResource {
                id: (*id).to_string(),
                type_name: "Script".to_string(),
                path: format!("res://script_{id}.gd"),
                uid: None,
            })
            .collect(),
        sub_resources: Vec::new(),
        nodes: vec![SceneNode {
            name: node_name.to_string(),
            type_name: Some("Node2D".to_string()),
            parent: None,
            instance: None,
            script: None,
            groups: Vec::new(),
            properties: Vec::new(),
        }],
        connections: Vec::new(),
    }
}

fn make_multi_node_scene() -> SceneData {
    SceneData {
        ext_resources: vec![ExtResource {
            id: "1_abc".to_string(),
            type_name: "Script".to_string(),
            path: "res://player.gd".to_string(),
            uid: None,
        }],
        sub_resources: Vec::new(),
        nodes: vec![
            SceneNode {
                name: "Root".to_string(),
                type_name: Some("Node3D".to_string()),
                parent: None,
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "Player".to_string(),
                type_name: Some("CharacterBody3D".to_string()),
                parent: Some(".".to_string()),
                instance: None,
                script: Some("1_abc".to_string()),
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "Sprite".to_string(),
                type_name: Some("Sprite2D".to_string()),
                parent: Some("Player".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
        ],
        connections: vec![Connection {
            signal: "ready".to_string(),
            from: "Player".to_string(),
            to: ".".to_string(),
            method: "_on_ready".to_string(),
        }],
    }
}

// ── next_ext_resource_id ────────────────────────────────────────────────────

#[test]
fn next_id_from_numeric_ids() {
    let data = make_scene_data(&["1", "2", "3"], "Root");
    assert_eq!(next_ext_resource_id(&data.ext_resources), "4");
}

#[test]
fn next_id_from_suffixed_ids() {
    let data = make_scene_data(&["1_abc", "2_def", "3_loading"], "Root");
    assert_eq!(next_ext_resource_id(&data.ext_resources), "4");
}

#[test]
fn next_id_empty_scene() {
    let data = make_scene_data(&[], "Root");
    assert_eq!(next_ext_resource_id(&data.ext_resources), "1");
}

// ── increment/decrement load_steps ──────────────────────────────────────────

#[test]
fn increment_load_steps_basic() {
    let line = r#"[gd_scene load_steps=3 format=3 uid="uid://abc"]"#;
    let result = increment_load_steps(line);
    assert!(result.contains("load_steps=4"));
}

#[test]
fn increment_load_steps_no_steps() {
    let line = r"[gd_scene format=3]";
    let result = increment_load_steps(line);
    assert_eq!(result, line);
}

#[test]
fn decrement_load_steps_basic() {
    let line = r"[gd_scene load_steps=5 format=3]";
    let result = decrement_load_steps(line, 2);
    assert!(result.contains("load_steps=3"));
}

#[test]
fn decrement_load_steps_saturates_at_zero() {
    let line = r"[gd_scene load_steps=1 format=3]";
    let result = decrement_load_steps(line, 5);
    assert!(result.contains("load_steps=0"));
}

// ── attach_script ───────────────────────────────────────────────────────────

#[test]
fn attach_script_to_root() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Texture2D" path="res://icon.png" id="1"]

[node name="Root" type="Node2D"]

[node name="Child" type="Sprite2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = insert_script_attachment(source, "res://root.gd", "2", &data.nodes[0]).unwrap();

    assert!(result.contains(r#"[ext_resource type="Script" path="res://root.gd" id="2"]"#));
    assert!(result.contains("load_steps=3"));
    let lines: Vec<&str> = result.lines().collect();
    let node_idx = lines
        .iter()
        .position(|l| l.contains("name=\"Root\""))
        .unwrap();
    assert_eq!(lines[node_idx + 1], r#"script = ExtResource("2")"#);
}

#[test]
fn attach_script_to_named_child() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = insert_script_attachment(source, "res://player.gd", "1", &data.nodes[1]).unwrap();

    assert!(result.contains(r#"[ext_resource type="Script" path="res://player.gd" id="1"]"#));
    let lines: Vec<&str> = result.lines().collect();
    let node_idx = lines
        .iter()
        .position(|l| l.contains("name=\"Player\""))
        .unwrap();
    assert_eq!(lines[node_idx + 1], r#"script = ExtResource("1")"#);
}

#[test]
fn attach_preserves_existing_ext_resources() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://existing.gd" id="1"]

[node name="Root" type="Node2D"]
script = ExtResource("1")

[node name="Enemy" type="CharacterBody2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = insert_script_attachment(source, "res://enemy.gd", "2", &data.nodes[1]).unwrap();

    assert!(result.contains(r#"path="res://existing.gd" id="1""#));
    assert!(result.contains(r#"path="res://enemy.gd" id="2""#));
    assert!(result.contains("load_steps=3"));
}

// ── compute_node_path ───────────────────────────────────────────────────────

#[test]
fn compute_node_path_root() {
    let data = make_multi_node_scene();
    assert_eq!(compute_node_path(&data.nodes[0], &data), ".");
}

#[test]
fn compute_node_path_direct_child() {
    let data = make_multi_node_scene();
    assert_eq!(compute_node_path(&data.nodes[1], &data), "Player");
}

#[test]
fn compute_node_path_nested_child() {
    let data = make_multi_node_scene();
    assert_eq!(compute_node_path(&data.nodes[2], &data), "Player/Sprite");
}

// ── parent_attr_for_node ────────────────────────────────────────────────────

#[test]
fn parent_attr_for_root() {
    let data = make_multi_node_scene();
    assert_eq!(parent_attr_for_node("Root", &data).unwrap(), ".");
}

#[test]
fn parent_attr_for_child() {
    let data = make_multi_node_scene();
    assert_eq!(parent_attr_for_node("Player", &data).unwrap(), "Player");
}

#[test]
fn parent_attr_for_nested() {
    let data = make_multi_node_scene();
    assert_eq!(
        parent_attr_for_node("Sprite", &data).unwrap(),
        "Player/Sprite"
    );
}

// ── extract_ext_resource_id ─────────────────────────────────────────────────

#[test]
fn extract_ext_resource_id_basic() {
    assert_eq!(
        extract_ext_resource_id(r#"ExtResource("1_abc")"#),
        Some("1_abc")
    );
}

#[test]
fn extract_ext_resource_id_none() {
    assert_eq!(extract_ext_resource_id("true"), None);
}

// ── clean_double_blanks ─────────────────────────────────────────────────────

#[test]
fn clean_double_blanks_removes_extra() {
    let input = "a\n\n\nb\n\nc\n";
    let result = clean_double_blanks(input);
    assert_eq!(result, "a\n\nb\n\nc\n");
}

// ── is_ext_resource_referenced ──────────────────────────────────────────────

#[test]
fn ext_resource_referenced_in_node() {
    let source = r#"[gd_scene format=3]

[ext_resource type="Script" path="res://test.gd" id="1_abc"]

[node name="Root" type="Node2D"]
script = ExtResource("1_abc")
"#;
    assert!(is_ext_resource_referenced(source, "1_abc"));
}

#[test]
fn ext_resource_not_referenced() {
    let source = r#"[gd_scene format=3]

[ext_resource type="Script" path="res://test.gd" id="1_abc"]

[node name="Root" type="Node2D"]
"#;
    assert!(!is_ext_resource_referenced(source, "1_abc"));
}

// ── create ──────────────────────────────────────────────────────────────────

#[test]
fn create_basic_scene() {
    let result = create::generate_scene("Node2D", "Root");
    assert!(result.contains("[gd_scene format=3]"));
    assert!(result.contains(r#"[node name="Root" type="Node2D"]"#));
    assert!(result.ends_with('\n'));
}

#[test]
fn create_pascal_case_name() {
    assert_eq!(create::to_pascal_case("main_menu"), "MainMenu");
    assert_eq!(create::to_pascal_case("game"), "Game");
    assert_eq!(
        create::to_pascal_case("player_hud_overlay"),
        "PlayerHudOverlay"
    );
    assert_eq!(create::to_pascal_case("already"), "Already");
}

// ── add_node ────────────────────────────────────────────────────────────────

#[test]
fn add_node_to_root() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_node::insert_node(source, &data, "Player", "CharacterBody2D", ".").unwrap();
    assert!(result.contains(r#"[node name="Player" type="CharacterBody2D" parent="."]"#));
}

#[test]
fn add_node_to_named_parent() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_node::insert_node(source, &data, "Sprite", "Sprite2D", "Player").unwrap();
    assert!(result.contains(r#"[node name="Sprite" type="Sprite2D" parent="Player"]"#));
}

#[test]
fn add_node_before_connections() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]

[connection signal="ready" from="Player" to="." method="_on_ready"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_node::insert_node(source, &data, "Enemy", "CharacterBody2D", ".").unwrap();
    let lines: Vec<&str> = result.lines().collect();
    let node_idx = lines
        .iter()
        .position(|l| l.contains("name=\"Enemy\""))
        .unwrap();
    let conn_idx = lines
        .iter()
        .position(|l| l.starts_with("[connection"))
        .unwrap();
    assert!(node_idx < conn_idx);
}

#[test]
fn add_node_duplicate_sibling_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_node::insert_node(source, &data, "Player", "Sprite2D", ".");
    assert!(result.is_err());
}

// ── set_property ────────────────────────────────────────────────────────────

#[test]
fn set_property_new() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
    let result =
        set_property::apply_set_property(source, "Player", Some("."), "visible", "false").unwrap();
    let lines: Vec<&str> = result.lines().collect();
    let node_idx = lines
        .iter()
        .position(|l| l.contains("name=\"Player\""))
        .unwrap();
    assert_eq!(lines[node_idx + 1], "visible = false");
}

#[test]
fn set_property_update_existing() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
visible = true
"#;
    let result =
        set_property::apply_set_property(source, "Player", Some("."), "visible", "false").unwrap();
    assert!(result.contains("visible = false"));
    // Should not have two visible lines
    assert_eq!(result.matches("visible").count(), 1);
}

#[test]
fn set_property_node_not_found() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let result = set_property::apply_set_property(source, "Nonexistent", Some("."), "key", "val");
    assert!(result.is_err());
}

#[test]
fn set_property_with_blank_line_before_existing() {
    // Bug: blank line between [node] header and existing properties caused duplicate
    let source = "[gd_scene format=3]\n\n\
                  [node name=\"Root\" type=\"Node2D\"]\n\n\
                  [node name=\"Player\" type=\"CharacterBody2D\" parent=\".\"]\n\
                  \n\
                  visible = true\n\
                  position = Vector2(100, 200)\n";
    let result =
        set_property::apply_set_property(source, "Player", Some("."), "visible", "false").unwrap();
    assert!(result.contains("visible = false"));
    // Must not have duplicates
    assert_eq!(
        result.matches("visible").count(),
        1,
        "should replace, not duplicate: {result}"
    );
    assert!(result.contains("position = Vector2(100, 200)"));
}

#[test]
fn set_property_replaces_multiline_value() {
    let source = "[gd_scene format=3]\n\n\
                  [node name=\"Root\" type=\"Node2D\"]\n\n\
                  [node name=\"Player\" type=\"CharacterBody2D\" parent=\".\"]\n\
                  data = [\n  \"a\",\n  \"b\"\n]\n\
                  speed = 100\n";
    let result =
        set_property::apply_set_property(source, "Player", Some("."), "data", "[\"new\"]").unwrap();
    assert!(result.contains("data = [\"new\"]"));
    assert!(!result.contains("\"a\""));
    assert!(!result.contains("\"b\""));
    assert!(result.contains("speed = 100"));
}

// ── add_connection ──────────────────────────────────────────────────────────

#[test]
fn add_connection_basic() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Button" type="Button" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_connection::insert_connection(
        source,
        &data,
        "pressed",
        "Button",
        ".",
        "_on_button_pressed",
    )
    .unwrap();
    assert!(result.contains(
        r#"[connection signal="pressed" from="Button" to="." method="_on_button_pressed"]"#
    ));
}

#[test]
fn add_connection_duplicate_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Button" type="Button" parent="."]

[connection signal="pressed" from="Button" to="." method="_on_pressed"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        add_connection::insert_connection(source, &data, "pressed", "Button", ".", "_on_pressed");
    assert!(result.is_err());
}

#[test]
fn add_connection_from_node_not_found() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        add_connection::insert_connection(source, &data, "pressed", "NoNode", ".", "_on_pressed");
    assert!(result.is_err());
}

// ── remove_connection ───────────────────────────────────────────────────────

#[test]
fn remove_connection_basic() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]

[connection signal="ready" from="Player" to="." method="_on_ready"]
"#;
    let result =
        remove_connection::remove_matching_connection(source, "ready", "Player", ".", "_on_ready")
            .unwrap();
    assert!(!result.contains("[connection"));
}

#[test]
fn remove_connection_not_found() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let result =
        remove_connection::remove_matching_connection(source, "ready", "Player", ".", "_on_ready");
    assert!(result.is_err());
}

#[test]
fn remove_connection_preserves_others() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]

[connection signal="ready" from="Player" to="." method="_on_ready"]

[connection signal="body_entered" from="Player" to="." method="_on_body"]
"#;
    let result =
        remove_connection::remove_matching_connection(source, "ready", "Player", ".", "_on_ready")
            .unwrap();
    assert!(!result.contains("_on_ready"));
    assert!(result.contains("_on_body"));
}

// ── detach_script ───────────────────────────────────────────────────────────

#[test]
fn detach_script_from_root() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://root.gd" id="1_abc"]

[node name="Root" type="Node2D"]
script = ExtResource("1_abc")
"#;
    let result = detach_script::apply_detach_script(source, "Root").unwrap();
    assert!(!result.contains("script ="));
    assert!(!result.contains("[ext_resource"));
    assert!(result.contains("load_steps=1"));
}

#[test]
fn detach_script_keeps_other_ext_resources() {
    let source = r#"[gd_scene load_steps=3 format=3]

[ext_resource type="Texture2D" path="res://icon.png" id="1"]

[ext_resource type="Script" path="res://root.gd" id="2"]

[node name="Root" type="Node2D"]
script = ExtResource("2")

[node name="Sprite" type="Sprite2D" parent="."]
texture = ExtResource("1")
"#;
    let result = detach_script::apply_detach_script(source, "Root").unwrap();
    assert!(!result.contains("script ="));
    assert!(!result.contains(r#"path="res://root.gd""#));
    assert!(result.contains(r#"path="res://icon.png""#));
    assert!(result.contains("load_steps=2"));
}

#[test]
fn detach_script_no_script_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let result = detach_script::apply_detach_script(source, "Root");
    assert!(result.is_err());
}

// ── remove_node ─────────────────────────────────────────────────────────────

#[test]
fn remove_node_simple() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
    let result = remove_node::apply_remove_node(source, "Player").unwrap();
    assert!(!result.contains("Player"));
    assert!(result.contains("Root"));
}

#[test]
fn remove_node_cascades_children() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]

[node name="Sprite" type="Sprite2D" parent="Player"]

[node name="CollisionShape" type="CollisionShape2D" parent="Player"]
"#;
    let result = remove_node::apply_remove_node(source, "Player").unwrap();
    assert!(!result.contains("Player"));
    assert!(!result.contains("Sprite"));
    assert!(!result.contains("CollisionShape"));
    assert!(result.contains("Root"));
}

#[test]
fn remove_node_cleans_connections() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]

[connection signal="ready" from="Player" to="." method="_on_ready"]
"#;
    let result = remove_node::apply_remove_node(source, "Player").unwrap();
    assert!(!result.contains("[connection"));
}

#[test]
fn remove_node_cleans_orphaned_ext_resource() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://player.gd" id="1_abc"]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
script = ExtResource("1_abc")
"#;
    let result = remove_node::apply_remove_node(source, "Player").unwrap();
    assert!(!result.contains("[ext_resource"));
    assert!(result.contains("load_steps=1"));
}

#[test]
fn remove_root_node_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let result = remove_node::apply_remove_node(source, "Root");
    assert!(result.is_err());
}

#[test]
fn remove_node_not_found_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let result = remove_node::apply_remove_node(source, "Nonexistent");
    assert!(result.is_err());
}

// ── find_node path support ──────────────────────────────────────────────────

fn make_nested_scene_data() -> SceneData {
    SceneData {
        ext_resources: Vec::new(),
        sub_resources: Vec::new(),
        nodes: vec![
            SceneNode {
                name: "Root".to_string(),
                type_name: Some("Control".to_string()),
                parent: None,
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "MarginContainer".to_string(),
                type_name: Some("MarginContainer".to_string()),
                parent: Some(".".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "VBoxContainer".to_string(),
                type_name: Some("VBoxContainer".to_string()),
                parent: Some("MarginContainer".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
        ],
        connections: Vec::new(),
    }
}

#[test]
fn find_node_by_path() {
    let data = make_nested_scene_data();
    let node = find_node(&data, "MarginContainer/VBoxContainer").unwrap();
    assert_eq!(node.name, "VBoxContainer");
}

#[test]
fn find_node_by_simple_name() {
    let data = make_nested_scene_data();
    let node = find_node(&data, "MarginContainer").unwrap();
    assert_eq!(node.name, "MarginContainer");
}

#[test]
fn find_node_root_by_dot() {
    let data = make_nested_scene_data();
    let node = find_node(&data, ".").unwrap();
    assert_eq!(node.name, "Root");
}

#[test]
fn find_node_ambiguous_error() {
    // Two nodes named "Btn" at different nesting levels (neither is a direct
    // child of root, so bare-name "Btn" is genuinely ambiguous).
    let data = SceneData {
        ext_resources: Vec::new(),
        sub_resources: Vec::new(),
        nodes: vec![
            SceneNode {
                name: "Root".to_string(),
                type_name: Some("Node2D".to_string()),
                parent: None,
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "PanelA".to_string(),
                type_name: Some("Panel".to_string()),
                parent: Some(".".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "Btn".to_string(),
                type_name: Some("Button".to_string()),
                parent: Some("PanelA".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "PanelB".to_string(),
                type_name: Some("Panel".to_string()),
                parent: Some(".".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
            SceneNode {
                name: "Btn".to_string(),
                type_name: Some("Button".to_string()),
                parent: Some("PanelB".to_string()),
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            },
        ],
        connections: Vec::new(),
    };
    let result = find_node(&data, "Btn");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Ambiguous"), "expected ambiguous error: {err}");
    // But using full path resolves unambiguously
    let node = find_node(&data, "PanelA/Btn").unwrap();
    assert_eq!(node.parent.as_deref(), Some("PanelA"));
}

#[test]
fn parent_attr_for_nested_path() {
    let data = make_nested_scene_data();
    assert_eq!(
        parent_attr_for_node("MarginContainer/VBoxContainer", &data).unwrap(),
        "MarginContainer/VBoxContainer"
    );
}

#[test]
fn add_node_to_nested_parent() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Control"]

[node name="MarginContainer" type="MarginContainer" parent="."]

[node name="VBoxContainer" type="VBoxContainer" parent="MarginContainer"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let parent_attr = parent_attr_for_node("MarginContainer/VBoxContainer", &data).unwrap();
    let result = add_node::insert_node(source, &data, "Label", "Label", &parent_attr).unwrap();
    assert!(
        result
            .contains(r#"[node name="Label" type="Label" parent="MarginContainer/VBoxContainer"]"#)
    );
}

#[test]
fn set_property_on_nested_node() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Control"]

[node name="MarginContainer" type="MarginContainer" parent="."]

[node name="VBoxContainer" type="VBoxContainer" parent="MarginContainer"]
"#;
    let result = set_property::apply_set_property(
        source,
        "VBoxContainer",
        Some("MarginContainer"),
        "visible",
        "false",
    )
    .unwrap();
    assert!(result.contains("visible = false"));
    let lines: Vec<&str> = result.lines().collect();
    let node_idx = lines
        .iter()
        .position(|l| l.contains("name=\"VBoxContainer\""))
        .unwrap();
    assert_eq!(lines[node_idx + 1], "visible = false");
}

// ── add_instance ────────────────────────────────────────────────────────────

#[test]
fn add_instance_basic() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        add_instance::insert_instance(source, &data, "Weapon", ".", "res://weapon.tscn", "1", true)
            .unwrap();
    assert!(
        result.contains(r#"[ext_resource type="PackedScene" path="res://weapon.tscn" id="1"]"#)
    );
    assert!(result.contains(r#"[node name="Weapon" parent="." instance=ExtResource("1")]"#));
    // Instance node line should NOT have type= attribute
    let weapon_line = result
        .lines()
        .find(|l| l.contains("name=\"Weapon\""))
        .unwrap();
    assert!(!weapon_line.contains("type="));
}

#[test]
fn add_instance_with_existing_ext_resources() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://root.gd" id="1_abc"]

[node name="Root" type="Node2D"]
script = ExtResource("1_abc")
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        add_instance::insert_instance(source, &data, "Enemy", ".", "res://enemy.tscn", "2", true)
            .unwrap();
    assert!(result.contains(r#"path="res://root.gd""#));
    assert!(result.contains(r#"path="res://enemy.tscn""#));
    assert!(result.contains("load_steps=3"));
}

#[test]
fn add_instance_reuses_existing_ext() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="PackedScene" path="res://weapon.tscn" id="1"]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_instance::insert_instance(
        source,
        &data,
        "Weapon",
        ".",
        "res://weapon.tscn",
        "1",
        false,
    )
    .unwrap();
    // Should not add a second ext_resource
    assert_eq!(result.matches("[ext_resource").count(), 1);
    // load_steps should NOT be incremented
    assert!(result.contains("load_steps=2"));
}

#[test]
fn add_instance_duplicate_name_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Weapon" type="Sprite2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        add_instance::insert_instance(source, &data, "Weapon", ".", "res://weapon.tscn", "1", true);
    assert!(result.is_err());
}

#[test]
fn add_instance_nested_parent() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="UI" type="Control" parent="."]

[node name="Panel" type="Panel" parent="UI"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = add_instance::insert_instance(
        source,
        &data,
        "Dialog",
        "UI/Panel",
        "res://dialog.tscn",
        "1",
        true,
    )
    .unwrap();
    assert!(result.contains(r#"parent="UI/Panel""#));
}

// ── add_sub_resource ────────────────────────────────────────────────────────

#[test]
fn add_sub_resource_basic() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let result = add_sub_resource::insert_sub_resource(
        source,
        "BoxShape3D",
        "BoxShape3D_1",
        &[("size".to_string(), "Vector3(1, 1, 1)".to_string())],
    );
    assert!(result.contains(r#"[sub_resource type="BoxShape3D" id="BoxShape3D_1"]"#));
    assert!(result.contains("size = Vector3(1, 1, 1)"));
    // Sub-resource should appear before nodes
    let sub_idx = result.find("[sub_resource").unwrap();
    let node_idx = result.find("[node").unwrap();
    assert!(sub_idx < node_idx);
}

#[test]
fn add_sub_resource_increments_load_steps() {
    let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://test.gd" id="1"]

[node name="Root" type="Node2D"]
"#;
    let result =
        add_sub_resource::insert_sub_resource(source, "StyleBoxFlat", "StyleBoxFlat_1", &[]);
    assert!(result.contains("load_steps=3"));
}

#[test]
fn add_sub_resource_with_node_assignment() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node3D"]

[node name="Col" type="CollisionShape3D" parent="."]
"#;
    // First insert the sub-resource
    let intermediate = add_sub_resource::insert_sub_resource(
        source,
        "BoxShape3D",
        "BoxShape3D_1",
        &[("size".to_string(), "Vector3(2, 2, 2)".to_string())],
    );
    // Then set the property on the node
    let result = set_property::apply_set_property(
        &intermediate,
        "Col",
        Some("."),
        "shape",
        r#"SubResource("BoxShape3D_1")"#,
    )
    .unwrap();
    assert!(result.contains(r#"shape = SubResource("BoxShape3D_1")"#));
}

// ── next_sub_resource_id ────────────────────────────────────────────────────

#[test]
fn next_sub_resource_id_empty() {
    assert_eq!(next_sub_resource_id(&[], "BoxShape3D"), "BoxShape3D_1");
}

#[test]
fn next_sub_resource_id_increments() {
    let subs = vec![
        crate::core::scene::SubResource {
            id: "BoxShape3D_1".to_string(),
            type_name: "BoxShape3D".to_string(),
            properties: Vec::new(),
        },
        crate::core::scene::SubResource {
            id: "BoxShape3D_2".to_string(),
            type_name: "BoxShape3D".to_string(),
            properties: Vec::new(),
        },
    ];
    assert_eq!(next_sub_resource_id(&subs, "BoxShape3D"), "BoxShape3D_3");
}

// ── batch_add ───────────────────────────────────────────────────────────────

#[test]
fn batch_add_multiple_nodes() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    // First add Player
    let source = add_node::insert_node(source, &data, "Player", "CharacterBody2D", ".").unwrap();
    let data = scene::parse_scene(&source).unwrap();
    // Then add Sprite as child of Player
    let result = add_node::insert_node(&source, &data, "Sprite", "Sprite2D", "Player").unwrap();
    assert!(result.contains(r#"name="Player""#));
    assert!(result.contains(r#"[node name="Sprite" type="Sprite2D" parent="Player"]"#));
}

#[test]
fn batch_add_parent_then_child() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let source = add_node::insert_node(source, &data, "UI", "Control", ".").unwrap();
    let data = scene::parse_scene(&source).unwrap();
    let result = add_node::insert_node(&source, &data, "Label", "Label", "UI").unwrap();
    assert!(result.contains(r#"[node name="UI" type="Control" parent="."]"#));
    assert!(result.contains(r#"[node name="Label" type="Label" parent="UI"]"#));
}

// ── duplicate_node ──────────────────────────────────────────────────────────

#[test]
fn duplicate_node_basic() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
speed = 100
health = 3
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        duplicate_node::apply_duplicate_node(source, &data, "Player", "Player2", None).unwrap();
    assert!(result.contains(r#"[node name="Player2" type="CharacterBody2D" parent="."]"#));
    // Duplicated node should have the same properties
    let lines: Vec<&str> = result.lines().collect();
    let dup_idx = lines
        .iter()
        .position(|l| l.contains("name=\"Player2\""))
        .unwrap();
    assert!(lines[dup_idx + 1].contains("speed = 100"));
    assert!(lines[dup_idx + 2].contains("health = 3"));
}

#[test]
fn duplicate_node_different_parent() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="UI" type="Control" parent="."]

[node name="Player" type="CharacterBody2D" parent="."]
speed = 100
"#;
    let data = scene::parse_scene(source).unwrap();
    let result =
        duplicate_node::apply_duplicate_node(source, &data, "Player", "PlayerUI", Some("UI"))
            .unwrap();
    assert!(result.contains(r#"[node name="PlayerUI" type="CharacterBody2D" parent="UI"]"#));
}

#[test]
fn duplicate_node_name_conflict_error() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = duplicate_node::apply_duplicate_node(source, &data, "Player", "Player", None);
    assert!(result.is_err());
}

#[test]
fn duplicate_node_cannot_dup_root() {
    let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]
"#;
    let data = scene::parse_scene(source).unwrap();
    let result = duplicate_node::apply_duplicate_node(source, &data, "Root", "Root2", None);
    assert!(result.is_err());
}
