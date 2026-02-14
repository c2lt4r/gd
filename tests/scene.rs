mod common;

use std::fs;
use tempfile::TempDir;

use common::gd_bin;

fn create_godot_project_with_scene(temp: &TempDir) {
    // Create a minimal Godot project with .tscn and .gd files
    fs::write(
        temp.path().join("project.godot"),
        "[gd_resource type=\"Environment\" format=3]\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("player.gd"),
        "extends CharacterBody3D\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("main.tscn"),
        r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://player.gd" id="1_abc"]

[node name="Root" type="Node3D"]

[node name="Player" type="CharacterBody3D" parent="."]
script = ExtResource("1_abc")

[connection signal="ready" from="Player" to="." method="_on_ready"]
"#,
    )
    .unwrap();
}

// ── gd check with .tscn ───────────────────────────────────────────────────

#[test]
fn test_check_tscn_valid_scene() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("check")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd check");

    assert!(
        output.status.success(),
        "gd check should succeed on valid project with .tscn: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_check_tscn_broken_res_path() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("project.godot"),
        "[gd_resource type=\"Environment\" format=3]\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("broken.tscn"),
        r#"[gd_scene format=3]

[ext_resource type="Script" path="res://nonexistent.gd" id="1_abc"]

[node name="Root" type="Node3D"]
script = ExtResource("1_abc")
"#,
    )
    .unwrap();

    let output = gd_bin()
        .arg("check")
        .arg("--format")
        .arg("json")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("broken resource path"),
        "should report broken resource path in JSON output: {stdout}"
    );
}

#[test]
fn test_check_tscn_orphaned_ext_resource() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("project.godot"),
        "[gd_resource type=\"Environment\" format=3]\n",
    )
    .unwrap();

    // Create a .gd file so the path isn't broken
    fs::write(temp.path().join("unused_script.gd"), "extends Node\n").unwrap();

    fs::write(
        temp.path().join("orphan.tscn"),
        r#"[gd_scene format=3]

[ext_resource type="Script" path="res://unused_script.gd" id="1_never_used"]

[node name="Root" type="Node3D"]
"#,
    )
    .unwrap();

    let output = gd_bin()
        .arg("check")
        .arg("--format")
        .arg("json")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("orphaned"),
        "should report orphaned ext_resource: {stdout}"
    );
}

#[test]
fn test_check_json_includes_tscn_errors() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("project.godot"),
        "[gd_resource type=\"Environment\" format=3]\n",
    )
    .unwrap();

    fs::write(
        temp.path().join("test.tscn"),
        r#"[gd_scene format=3]

[ext_resource type="Texture" path="res://missing.png" id="tex_1"]

[node name="Root" type="Sprite2D"]
"#,
    )
    .unwrap();

    let output = gd_bin()
        .arg("check")
        .arg("--format")
        .arg("json")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("gd check --format json should output valid JSON");

    // The JSON should have the standard check output structure
    assert!(
        json.get("files_checked").is_some(),
        "should have files_checked"
    );
    assert!(json.get("errors").is_some(), "should have errors array");
}

// ── gd deps --include-resources ───────────────────────────────────────────

#[test]
fn test_deps_include_resources() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("deps")
        .arg("--include-resources")
        .arg("--format")
        .arg("json")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd deps");

    assert!(
        output.status.success(),
        "gd deps --include-resources should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("deps JSON output should be valid");

    // The .tscn file should appear in the dependency map
    let deps = json.get("dependencies").and_then(|d| d.as_object());
    assert!(deps.is_some(), "should have dependencies object");

    let deps = deps.unwrap();
    let has_tscn_entry = deps.keys().any(|k| {
        std::path::Path::new(k)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("tscn"))
    });
    assert!(has_tscn_entry, "dependency map should include .tscn files");

    // The .tscn should depend on player.gd
    let tscn_deps = deps.values().find(|v| {
        v.as_array().is_some_and(|arr| {
            arr.iter()
                .any(|d| d.as_str().is_some_and(|s| s.contains("player.gd")))
        })
    });
    assert!(tscn_deps.is_some(), "main.tscn should depend on player.gd");
}

// ── gd tree --scene ───────────────────────────────────────────────────────

#[test]
fn test_tree_scene_single_file() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("tree")
        .arg("--scene")
        .arg(temp.path().join("main.tscn"))
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to run gd tree --scene");

    assert!(
        output.status.success(),
        "gd tree --scene should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("tree --scene JSON should be valid");

    // Root node should be "Root"
    assert_eq!(
        json.get("name").and_then(|n| n.as_str()),
        Some("Root"),
        "root node should be 'Root'"
    );

    // Should have children
    let children = json.get("children").and_then(|c| c.as_array());
    assert!(children.is_some(), "root should have children");
    assert!(
        !children.unwrap().is_empty(),
        "root should have at least one child"
    );

    // First child should be "Player"
    let first_child = &children.unwrap()[0];
    assert_eq!(
        first_child.get("name").and_then(|n| n.as_str()),
        Some("Player"),
        "first child should be Player"
    );
}

#[test]
fn test_tree_scene_directory() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("tree")
        .arg("--scene")
        .arg(temp.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("Failed to run gd tree --scene (dir)");

    assert!(
        output.status.success(),
        "gd tree --scene on directory should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("tree --scene dir JSON should be valid");

    // Should be an array of scene entries
    assert!(json.is_array(), "directory output should be a JSON array");
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty(), "should have at least one scene");
    assert!(
        arr[0].get("file").is_some(),
        "each entry should have a file field"
    );
    assert!(
        arr[0].get("root").is_some(),
        "each entry should have a root field"
    );
}

#[test]
fn test_tree_scene_human_output() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("tree")
        .arg("--scene")
        .arg(temp.path().join("main.tscn"))
        .output()
        .expect("Failed to run gd tree --scene");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Human output should show the node names
    assert!(
        stdout.contains("Root"),
        "human output should show Root node"
    );
    assert!(
        stdout.contains("Player"),
        "human output should show Player node"
    );
}

// ── gd lsp scene-info ────────────────────────────────────────────────────

#[test]
fn test_lsp_scene_info() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("lsp")
        .arg("scene-info")
        .arg("--file")
        .arg(temp.path().join("main.tscn"))
        .output()
        .expect("Failed to run gd lsp scene-info");

    assert!(
        output.status.success(),
        "gd lsp scene-info should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("scene-info JSON should be valid");

    // Should have all four sections
    assert!(json.get("nodes").is_some(), "should have nodes");
    assert!(
        json.get("ext_resources").is_some(),
        "should have ext_resources"
    );
    assert!(json.get("connections").is_some(), "should have connections");

    // Nodes should include Root and Player
    let nodes = json["nodes"].as_array().unwrap();
    let names: Vec<&str> = nodes
        .iter()
        .filter_map(|n| n.get("name").and_then(|n| n.as_str()))
        .collect();
    assert!(names.contains(&"Root"), "nodes should contain Root");
    assert!(names.contains(&"Player"), "nodes should contain Player");
}

#[test]
fn test_lsp_scene_info_nodes_only() {
    let temp = TempDir::new().unwrap();
    create_godot_project_with_scene(&temp);

    let output = gd_bin()
        .arg("lsp")
        .arg("scene-info")
        .arg("--file")
        .arg(temp.path().join("main.tscn"))
        .arg("--nodes-only")
        .output()
        .expect("Failed to run gd lsp scene-info --nodes-only");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Should have nodes but NOT ext_resources, sub_resources, connections
    assert!(json.get("nodes").is_some(), "should have nodes");
    assert!(
        json.get("ext_resources").is_none(),
        "nodes-only should omit ext_resources"
    );
    assert!(
        json.get("connections").is_none(),
        "nodes-only should omit connections"
    );
}
