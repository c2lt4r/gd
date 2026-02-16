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

// ── gd scene create ──────────────────────────────────────────────────────

#[test]
fn test_scene_create_basic() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("level.tscn");

    let output = gd_bin()
        .arg("scene")
        .arg("create")
        .arg(&scene_path)
        .arg("--root-type")
        .arg("Node2D")
        .output()
        .expect("Failed to run gd scene create");

    assert!(
        output.status.success(),
        "gd scene create should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains("[gd_scene format=3]"));
    assert!(content.contains(r#"[node name="Level" type="Node2D"]"#));
}

#[test]
fn test_scene_create_custom_root_name() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");

    let output = gd_bin()
        .arg("scene")
        .arg("create")
        .arg(&scene_path)
        .arg("--root-type")
        .arg("Control")
        .arg("--root-name")
        .arg("MainMenu")
        .output()
        .expect("Failed to run gd scene create");

    assert!(output.status.success());
    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains(r#"[node name="MainMenu" type="Control"]"#));
}

#[test]
fn test_scene_create_already_exists() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("existing.tscn");
    fs::write(&scene_path, "[gd_scene format=3]\n").unwrap();

    let output = gd_bin()
        .arg("scene")
        .arg("create")
        .arg(&scene_path)
        .arg("--root-type")
        .arg("Node2D")
        .output()
        .expect("Failed to run gd scene create");

    assert!(
        !output.status.success(),
        "should fail if file already exists"
    );
}

#[test]
fn test_scene_create_dry_run() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("dryrun.tscn");

    let output = gd_bin()
        .arg("scene")
        .arg("create")
        .arg(&scene_path)
        .arg("--root-type")
        .arg("Node3D")
        .arg("--dry-run")
        .output()
        .expect("Failed to run gd scene create --dry-run");

    assert!(output.status.success());
    assert!(!scene_path.exists(), "dry-run should not create the file");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[gd_scene format=3]"));
}

// ── gd scene add-node ────────────────────────────────────────────────────

#[test]
fn test_scene_add_node() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        "[gd_scene format=3]\n\n[node name=\"Root\" type=\"Node2D\"]\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("scene")
        .arg("add-node")
        .arg(&scene_path)
        .arg("--name")
        .arg("Player")
        .arg("--type")
        .arg("CharacterBody2D")
        .output()
        .expect("Failed to run gd scene add-node");

    assert!(
        output.status.success(),
        "gd scene add-node should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains(r#"[node name="Player" type="CharacterBody2D" parent="."]"#));
}

#[test]
fn test_scene_add_node_to_parent() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#,
    )
    .unwrap();

    let output = gd_bin()
        .arg("scene")
        .arg("add-node")
        .arg(&scene_path)
        .arg("--name")
        .arg("Sprite")
        .arg("--type")
        .arg("Sprite2D")
        .arg("--parent")
        .arg("Player")
        .output()
        .expect("Failed to run gd scene add-node --parent");

    assert!(output.status.success());
    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains(r#"[node name="Sprite" type="Sprite2D" parent="Player"]"#));
}

// ── gd scene set-property ────────────────────────────────────────────────

#[test]
fn test_scene_set_property() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#,
    )
    .unwrap();

    let output = gd_bin()
        .arg("scene")
        .arg("set-property")
        .arg(&scene_path)
        .arg("--node")
        .arg("Player")
        .arg("--key")
        .arg("visible")
        .arg("--value")
        .arg("false")
        .output()
        .expect("Failed to run gd scene set-property");

    assert!(
        output.status.success(),
        "gd scene set-property should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains("visible = false"));
}

// ── gd scene add-connection / remove-connection ──────────────────────────

#[test]
fn test_scene_add_and_remove_connection() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Button" type="Button" parent="."]
"#,
    )
    .unwrap();

    // Add connection
    let output = gd_bin()
        .arg("scene")
        .arg("add-connection")
        .arg(&scene_path)
        .arg("--signal")
        .arg("pressed")
        .arg("--from")
        .arg("Button")
        .arg("--to")
        .arg(".")
        .arg("--method")
        .arg("_on_button_pressed")
        .output()
        .expect("Failed to run gd scene add-connection");

    assert!(
        output.status.success(),
        "gd scene add-connection should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains(
        r#"[connection signal="pressed" from="Button" to="." method="_on_button_pressed"]"#
    ));

    // Remove connection
    let output = gd_bin()
        .arg("scene")
        .arg("remove-connection")
        .arg(&scene_path)
        .arg("--signal")
        .arg("pressed")
        .arg("--from")
        .arg("Button")
        .arg("--to")
        .arg(".")
        .arg("--method")
        .arg("_on_button_pressed")
        .output()
        .expect("Failed to run gd scene remove-connection");

    assert!(
        output.status.success(),
        "gd scene remove-connection should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(!content.contains("[connection"));
}

// ── gd scene attach-script / detach-script ───────────────────────────────

#[test]
fn test_scene_attach_and_detach_script() {
    let temp = TempDir::new().unwrap();

    // Need project.godot for attach-script
    fs::write(
        temp.path().join("project.godot"),
        "[gd_resource type=\"Environment\" format=3]\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("root.gd"),
        "extends Node2D\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .unwrap();

    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        "[gd_scene format=3]\n\n[node name=\"Root\" type=\"Node2D\"]\n",
    )
    .unwrap();

    // Attach
    let output = gd_bin()
        .arg("scene")
        .arg("attach-script")
        .arg(&scene_path)
        .arg("root.gd")
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd scene attach-script");

    assert!(
        output.status.success(),
        "gd scene attach-script should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains("script = ExtResource("));
    assert!(content.contains(r#"path="res://root.gd""#));

    // Detach
    let output = gd_bin()
        .arg("scene")
        .arg("detach-script")
        .arg(&scene_path)
        .output()
        .expect("Failed to run gd scene detach-script");

    assert!(
        output.status.success(),
        "gd scene detach-script should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(!content.contains("script ="));
    assert!(!content.contains("[ext_resource"));
}

#[test]
fn test_scene_attach_script_from_subdirectory() {
    let temp = TempDir::new().unwrap();

    fs::write(
        temp.path().join("project.godot"),
        "[gd_resource type=\"Environment\" format=3]\n",
    )
    .unwrap();

    // Script in a subdirectory
    fs::create_dir_all(temp.path().join("scripts")).unwrap();
    fs::write(
        temp.path().join("scripts/player.gd"),
        "extends Node2D\n\nfunc _ready():\n\tpass\n",
    )
    .unwrap();

    let scene_path = temp.path().join("level.tscn");
    fs::write(
        &scene_path,
        "[gd_scene format=3]\n\n[node name=\"Root\" type=\"Node2D\"]\n",
    )
    .unwrap();

    // Run from a subdirectory, pass paths relative to project root
    fs::create_dir_all(temp.path().join("subdir")).unwrap();
    let output = gd_bin()
        .arg("scene")
        .arg("attach-script")
        .arg("level.tscn")
        .arg("scripts/player.gd")
        .current_dir(temp.path().join("subdir"))
        .output()
        .expect("Failed to run gd scene attach-script");

    assert!(
        output.status.success(),
        "attach-script from subdirectory should resolve paths via project root: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains("script = ExtResource("));
    assert!(content.contains(r#"path="res://scripts/player.gd""#));
}

// ── gd scene remove-node ─────────────────────────────────────────────────

#[test]
fn test_scene_remove_node_cascades() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]

[node name="Sprite" type="Sprite2D" parent="Player"]

[connection signal="ready" from="Player" to="." method="_on_ready"]
"#,
    )
    .unwrap();

    let output = gd_bin()
        .arg("scene")
        .arg("remove-node")
        .arg(&scene_path)
        .arg("--name")
        .arg("Player")
        .output()
        .expect("Failed to run gd scene remove-node");

    assert!(
        output.status.success(),
        "gd scene remove-node should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&scene_path).unwrap();
    assert!(content.contains("Root"));
    assert!(!content.contains("Player"));
    assert!(!content.contains("Sprite"));
    assert!(!content.contains("[connection"));
}

#[test]
fn test_scene_remove_node_cannot_remove_root() {
    let temp = TempDir::new().unwrap();
    let scene_path = temp.path().join("test.tscn");
    fs::write(
        &scene_path,
        "[gd_scene format=3]\n\n[node name=\"Root\" type=\"Node2D\"]\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("scene")
        .arg("remove-node")
        .arg(&scene_path)
        .arg("--name")
        .arg("Root")
        .output()
        .expect("Failed to run gd scene remove-node");

    assert!(
        !output.status.success(),
        "should fail when trying to remove root node"
    );
}

// ── gd scene full workflow ───────────────────────────────────────────────

/// Run `gd scene <subcmd> <scene_path> [extra_args...]` and assert success.
fn run_scene_cmd(subcmd: &str, scene: &std::path::Path, extra: &[&str], label: &str) {
    let mut cmd = gd_bin();
    cmd.arg("scene").arg(subcmd).arg(scene);
    for arg in extra {
        cmd.arg(arg);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_scene_full_workflow_build() {
    let temp = TempDir::new().unwrap();
    let s = temp.path().join("game.tscn");

    run_scene_cmd("create", &s, &["--root-type", "Node2D"], "create");
    run_scene_cmd(
        "add-node",
        &s,
        &["--name", "Player", "--type", "CharacterBody2D"],
        "add Player",
    );
    run_scene_cmd(
        "add-node",
        &s,
        &[
            "--name", "Sprite", "--type", "Sprite2D", "--parent", "Player",
        ],
        "add Sprite",
    );
    run_scene_cmd(
        "set-property",
        &s,
        &["--node", "Player", "--key", "visible", "--value", "false"],
        "set-property",
    );
    run_scene_cmd(
        "add-connection",
        &s,
        &[
            "--signal",
            "ready",
            "--from",
            "Player",
            "--to",
            ".",
            "--method",
            "_on_ready",
        ],
        "add-connection",
    );

    let c = fs::read_to_string(&s).unwrap();
    assert!(c.contains("Game") && c.contains("Player") && c.contains("Sprite"));
    assert!(c.contains("visible = false"));
    assert!(c.contains("[connection"));
}

#[test]
fn test_scene_full_workflow_teardown() {
    let temp = TempDir::new().unwrap();
    let s = temp.path().join("game.tscn");

    run_scene_cmd("create", &s, &["--root-type", "Node2D"], "create");
    run_scene_cmd(
        "add-node",
        &s,
        &["--name", "Player", "--type", "CharacterBody2D"],
        "add Player",
    );
    run_scene_cmd(
        "add-node",
        &s,
        &[
            "--name", "Sprite", "--type", "Sprite2D", "--parent", "Player",
        ],
        "add Sprite",
    );
    run_scene_cmd(
        "add-connection",
        &s,
        &[
            "--signal",
            "ready",
            "--from",
            "Player",
            "--to",
            ".",
            "--method",
            "_on_ready",
        ],
        "add-connection",
    );

    run_scene_cmd(
        "remove-connection",
        &s,
        &[
            "--signal",
            "ready",
            "--from",
            "Player",
            "--to",
            ".",
            "--method",
            "_on_ready",
        ],
        "remove-connection",
    );
    run_scene_cmd("remove-node", &s, &["--name", "Player"], "remove Player");

    let c = fs::read_to_string(&s).unwrap();
    assert!(c.contains("Game"), "should contain Game root: {c}");
    assert!(!c.contains("Player"));
    assert!(!c.contains("Sprite"));
    assert!(!c.contains("[connection"));
}

// ── gd lsp scene-info ────────────────────────────────────────────────────

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
