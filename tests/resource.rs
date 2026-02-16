mod common;

use std::fs;
use tempfile::TempDir;

use common::gd_bin;

// ── gd resource create ──────────────────────────────────────────────────────

#[test]
fn test_resource_create_basic() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("item.tres");

    let output = gd_bin()
        .arg("resource")
        .arg("create")
        .arg(&path)
        .arg("--type")
        .arg("Resource")
        .output()
        .expect("Failed to run gd resource create");

    assert!(
        output.status.success(),
        "gd resource create should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("[gd_resource type=\"Resource\" format=3]"));
    assert!(content.contains("[resource]"));
}

#[test]
fn test_resource_create_already_exists() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("existing.tres");
    fs::write(&path, "[gd_resource type=\"Resource\" format=3]\n").unwrap();

    let output = gd_bin()
        .arg("resource")
        .arg("create")
        .arg(&path)
        .arg("--type")
        .arg("Resource")
        .output()
        .expect("Failed to run gd resource create");

    assert!(
        !output.status.success(),
        "should fail if file already exists"
    );
}

#[test]
fn test_resource_create_dry_run() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dryrun.tres");

    let output = gd_bin()
        .arg("resource")
        .arg("create")
        .arg(&path)
        .arg("--type")
        .arg("Theme")
        .arg("--dry-run")
        .output()
        .expect("Failed to run gd resource create --dry-run");

    assert!(output.status.success());
    assert!(!path.exists(), "dry-run should not create the file");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[gd_resource type=\"Theme\" format=3]"));
}

#[test]
fn test_resource_create_with_script() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("item_data.gd"),
        "extends Resource\n\n@export var cost: int = 0\n",
    )
    .unwrap();

    let path = temp.path().join("item.tres");

    let output = gd_bin()
        .arg("resource")
        .arg("create")
        .arg(&path)
        .arg("--type")
        .arg("Resource")
        .arg("--script")
        .arg("item_data.gd")
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd resource create --script");

    assert!(
        output.status.success(),
        "gd resource create --script should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("load_steps=2"));
    assert!(content.contains(r#"path="res://item_data.gd""#));
    assert!(content.contains(r#"script = ExtResource("1")"#));
}

// ── gd resource set-property / get-property / remove-property ───────────────

#[test]
fn test_resource_property_workflow() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("data.tres");
    fs::write(
        &path,
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\n",
    )
    .unwrap();

    // Set a property
    let output = gd_bin()
        .arg("resource")
        .arg("set-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .arg("--value")
        .arg("100")
        .output()
        .expect("Failed to run gd resource set-property");

    assert!(
        output.status.success(),
        "set-property should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Get the property
    let output = gd_bin()
        .arg("resource")
        .arg("get-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .output()
        .expect("Failed to run gd resource get-property");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "100");

    // Update the property
    let output = gd_bin()
        .arg("resource")
        .arg("set-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .arg("--value")
        .arg("200")
        .output()
        .expect("Failed to run gd resource set-property (update)");

    assert!(output.status.success());

    let output = gd_bin()
        .arg("resource")
        .arg("get-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .output()
        .expect("Failed to run gd resource get-property (updated)");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "200");

    // Remove the property
    let output = gd_bin()
        .arg("resource")
        .arg("remove-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .output()
        .expect("Failed to run gd resource remove-property");

    assert!(
        output.status.success(),
        "remove-property should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify it's gone
    let output = gd_bin()
        .arg("resource")
        .arg("get-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .output()
        .expect("Failed to run gd resource get-property (removed)");

    assert!(
        !output.status.success(),
        "get-property should fail for removed key"
    );
}

#[test]
fn test_resource_get_property_not_found() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("data.tres");
    fs::write(
        &path,
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("resource")
        .arg("get-property")
        .arg(&path)
        .arg("--key")
        .arg("nonexistent")
        .output()
        .expect("Failed to run gd resource get-property");

    assert!(
        !output.status.success(),
        "should fail for nonexistent property"
    );
}

// ── gd resource info ────────────────────────────────────────────────────────

#[test]
fn test_resource_info() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("theme.tres");
    fs::write(
        &path,
        "[gd_resource type=\"Theme\" format=3]\n\n[resource]\ndefault_font_size = 16\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("resource")
        .arg("info")
        .arg(&path)
        .args(["--format", "json"])
        .output()
        .expect("Failed to run gd resource info");

    assert!(
        output.status.success(),
        "gd resource info should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should output valid JSON");

    assert_eq!(
        json.get("type_name").and_then(|v| v.as_str()),
        Some("Theme")
    );
    assert!(json.get("properties").is_some());
}

// ── gd resource set-script / remove-script ──────────────────────────────────

#[test]
fn test_resource_script_workflow() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("item_data.gd"),
        "extends Resource\n\n@export var cost: int = 0\n",
    )
    .unwrap();

    let path = temp.path().join("item.tres");
    fs::write(
        &path,
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n",
    )
    .unwrap();

    // Set script
    let output = gd_bin()
        .arg("resource")
        .arg("set-script")
        .arg(&path)
        .arg("item_data.gd")
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd resource set-script");

    assert!(
        output.status.success(),
        "set-script should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains(r#"path="res://item_data.gd""#));
    assert!(content.contains("script = ExtResource("));
    assert!(content.contains("load_steps="));

    // Remove script
    let output = gd_bin()
        .arg("resource")
        .arg("remove-script")
        .arg(&path)
        .output()
        .expect("Failed to run gd resource remove-script");

    assert!(
        output.status.success(),
        "remove-script should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&path).unwrap();
    assert!(!content.contains("script ="));
    assert!(!content.contains("[ext_resource"));
}

#[test]
fn test_resource_remove_script_no_script() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("plain.tres");
    fs::write(
        &path,
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("resource")
        .arg("remove-script")
        .arg(&path)
        .output()
        .expect("Failed to run gd resource remove-script");

    assert!(
        !output.status.success(),
        "should fail when no script is attached"
    );
}

// ── gd resource set-property --dry-run ──────────────────────────────────────

#[test]
fn test_resource_set_property_dry_run() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("data.tres");
    let original = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n";
    fs::write(&path, original).unwrap();

    let output = gd_bin()
        .arg("resource")
        .arg("set-property")
        .arg(&path)
        .arg("--key")
        .arg("cost")
        .arg("--value")
        .arg("999")
        .arg("--dry-run")
        .output()
        .expect("Failed to run gd resource set-property --dry-run");

    assert!(output.status.success());

    // File should be unchanged
    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, original);

    // Stdout should contain the modified version
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cost = 999"));
}
