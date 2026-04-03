mod common;

use common::{gd_bin, setup_gd_project};

#[test]
fn test_parse_valid_file() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = gd_bin()
        .args(["parse"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd parse");

    assert!(
        output.status.success(),
        "gd parse should succeed on valid files, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_parse_syntax_error() {
    let temp = setup_gd_project(&[("broken.gd", "func (:\n\t\tif if if\n")]);

    let output = gd_bin()
        .args(["parse"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd parse");

    assert!(
        !output.status.success(),
        "gd parse should fail on syntax errors"
    );
}

#[test]
fn test_parse_does_not_reject_semantic_errors() {
    // Cyclic const references are semantically invalid but syntactically fine.
    // gd parse should accept them (unlike gd check which does semantic analysis).
    let temp = setup_gd_project(&[(
        "cyclic.gd",
        "const A = B\nconst B = A\n\n\nfunc test():\n\tprint(A)\n",
    )]);

    let output = gd_bin()
        .args(["parse"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd parse");

    assert!(
        output.status.success(),
        "gd parse should not reject semantic-only errors, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_parse_json_no_errors() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = gd_bin()
        .args(["parse", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd parse --format json");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["files_with_errors"], 0);
    assert!(json["files_parsed"].as_u64().unwrap() > 0);
}

#[test]
fn test_parse_json_with_errors() {
    let temp = setup_gd_project(&[("broken.gd", "func (:\n\t\tif if if\n")]);

    let output = gd_bin()
        .args(["parse", "--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd parse --format json");

    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert!(json["files_with_errors"].as_u64().unwrap() > 0);
    assert!(!json["errors"].as_array().unwrap().is_empty());

    let first_error = &json["errors"][0];
    assert!(first_error["file"].as_str().is_some());
    assert!(first_error["line"].as_u64().unwrap() > 0);
    assert_eq!(first_error["message"], "parse error");
}

#[test]
fn test_parse_explicit_path() {
    let temp = setup_gd_project(&[("valid.gd", "extends Node\n")]);

    let output = gd_bin()
        .args(["parse", &temp.path().join("valid.gd").to_string_lossy()])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd parse with path");

    assert!(output.status.success());
}
