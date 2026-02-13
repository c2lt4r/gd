mod common;

use std::fs;

use common::{gd_bin, setup_gd_project};

// ─── check command ───────────────────────────────────────────────────────────

#[test]
fn test_check_respects_ignore_patterns() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // gd.toml with ignore_patterns
    fs::write(
        temp.path().join("gd.toml"),
        "[lint]\nignore_patterns = [\"vendor/**\"]\n",
    )
    .expect("write gd.toml");

    // Create vendor/ directory with a broken file
    let vendor = temp.path().join("vendor");
    fs::create_dir_all(&vendor).expect("create vendor dir");
    fs::write(vendor.join("broken.gd"), "func (:\n\t\tif if if\n").expect("write vendor/broken.gd");

    // Create a clean root file
    fs::write(
        temp.path().join("main.gd"),
        "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .expect("write main.gd");

    let output = gd_bin()
        .arg("check")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd check");

    assert!(
        output.status.success(),
        "gd check should pass when broken files are in ignored dirs, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_check_json_no_errors() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = gd_bin()
        .args(["check", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd check --format json");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["files_with_errors"], 0);
    assert!(json["files_checked"].as_u64().unwrap() > 0);
    assert!(json["errors"].as_array().unwrap().is_empty());
}

#[test]
fn test_check_json_with_errors() {
    let temp = setup_gd_project(&[("broken.gd", "func (:\n\t\tif if if\n")]);

    let output = gd_bin()
        .args(["check", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd check --format json");

    assert!(!output.status.success(), "should exit non-zero on errors");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert!(json["files_with_errors"].as_u64().unwrap() > 0);
    assert!(!json["errors"].as_array().unwrap().is_empty());

    let first_error = &json["errors"][0];
    assert!(first_error["file"].as_str().is_some());
    assert!(first_error["line"].as_u64().unwrap() > 0);
}
