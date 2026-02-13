mod common;

use std::fs;

use common::gd_bin;

fn create_deps_project(temp: &std::path::Path) {
    fs::write(
        temp.join("project.godot"),
        "[application]\nconfig/name=\"test-deps\"\n",
    )
    .expect("write project.godot");

    fs::write(
        temp.join("base.gd"),
        "extends Node\n\nfunc greet() -> void:\n\tprint(\"hello\")\n",
    )
    .expect("write base.gd");

    fs::write(
        temp.join("child.gd"),
        "extends \"res://base.gd\"\n\nvar helper = preload(\"res://helper.gd\")\n\nfunc _ready() -> void:\n\tgreet()\n",
    )
    .expect("write child.gd");

    fs::write(
        temp.join("helper.gd"),
        "extends RefCounted\n\nfunc help() -> void:\n\tpass\n",
    )
    .expect("write helper.gd");
}

// ─── deps command ────────────────────────────────────────────────────────────

#[test]
fn test_deps_tree_output() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    create_deps_project(temp.path());

    let output = gd_bin()
        .arg("deps")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd deps");

    assert!(output.status.success(), "gd deps should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // child.gd depends on base.gd and helper.gd via extends/preload
    assert!(
        stdout.contains("child.gd"),
        "deps tree should list child.gd, got: {}",
        stdout
    );
    assert!(
        stdout.contains("res://base.gd") || stdout.contains("base.gd"),
        "deps tree should show base.gd dependency, got: {}",
        stdout
    );
}

#[test]
fn test_deps_dot_output() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    create_deps_project(temp.path());

    let output = gd_bin()
        .arg("deps")
        .arg("--format")
        .arg("dot")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd deps --format dot");

    assert!(
        output.status.success(),
        "gd deps --format dot should succeed"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("digraph"),
        "DOT output should contain 'digraph', got: {}",
        stdout
    );
    assert!(
        stdout.contains("->"),
        "DOT output should contain edges (->), got: {}",
        stdout
    );
}

#[test]
fn test_deps_json_output() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    create_deps_project(temp.path());

    let output = gd_bin()
        .arg("deps")
        .arg("--format")
        .arg("json")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd deps --format json");

    assert!(
        output.status.success(),
        "gd deps --format json should succeed"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("gd deps --format json should produce valid JSON");

    assert!(
        json["files"].as_u64().unwrap() >= 3,
        "Should report at least 3 files"
    );
    assert!(
        json["dependencies"].is_object(),
        "Should have dependencies object"
    );
}

#[test]
fn test_deps_cycle_detection() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"cycle-test\"\n",
    )
    .expect("write project.godot");

    fs::write(temp.path().join("a.gd"), "extends \"b.gd\"\n").expect("write a.gd");

    fs::write(temp.path().join("b.gd"), "extends \"a.gd\"\n").expect("write b.gd");

    let output = gd_bin()
        .arg("deps")
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd deps");

    assert!(
        !output.status.success(),
        "gd deps should fail when circular dependency exists"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("circular"),
        "Should report circular dependency, stderr: {}",
        stderr
    );
}

#[test]
fn test_deps_no_cycle_check() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"cycle-test\"\n",
    )
    .expect("write project.godot");

    fs::write(temp.path().join("a.gd"), "extends \"b.gd\"\n").expect("write a.gd");

    fs::write(temp.path().join("b.gd"), "extends \"a.gd\"\n").expect("write b.gd");

    let output = gd_bin()
        .arg("deps")
        .arg("--no-cycle-check")
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd deps --no-cycle-check");

    assert!(
        output.status.success(),
        "gd deps --no-cycle-check should succeed even with cycles, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("circular"),
        "Should not report circular dependency with --no-cycle-check, stderr: {}",
        stderr
    );
}
