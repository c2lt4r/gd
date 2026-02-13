mod common;

use std::fs;
use tempfile::TempDir;

use common::gd_bin;

#[test]
fn test_fmt_check_clean_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("clean.gd");

    // Write a well-formatted file (using real tabs)
    fs::write(
        &file_path,
        "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("fmt")
        .arg("--check")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt --check");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd fmt --check should return 0 for clean file"
    );
}

#[test]
fn test_fmt_check_unformatted_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("messy.gd");

    // Write an unformatted file (extra spaces, wrong indentation)
    fs::write(
        &file_path,
        "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("fmt")
        .arg("--check")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt --check");

    assert_ne!(
        output.status.code(),
        Some(0),
        "gd fmt --check should return non-zero for unformatted file"
    );
}

#[test]
fn test_fmt_diff() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("diff_test.gd");

    // Write an unformatted file
    fs::write(
        &file_path,
        "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("fmt")
        .arg("--diff")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt --diff");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("---") && stdout.contains("+++"),
        "gd fmt --diff should show unified diff markers"
    );
}

// ─── Formatter edge cases ───────────────────────────────────────────────────

#[test]
fn test_fmt_multiple_files() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // Write two unformatted files
    fs::write(
        temp.path().join("a.gd"),
        "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n",
    )
    .expect("write a.gd");
    fs::write(
        temp.path().join("b.gd"),
        "extends Node2D\n\n\n\n\nfunc _process(delta)->void:\n  pass\n",
    )
    .expect("write b.gd");

    // Format the whole directory
    let output = gd_bin()
        .arg("fmt")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd fmt");

    assert!(
        output.status.success(),
        "gd fmt should succeed on directory"
    );

    // Both files should now be formatted (tabs, not spaces)
    let a = fs::read_to_string(temp.path().join("a.gd")).unwrap();
    let b = fs::read_to_string(temp.path().join("b.gd")).unwrap();
    assert!(
        a.contains("\tpass"),
        "a.gd should use tab indentation after fmt"
    );
    assert!(
        b.contains("\tpass"),
        "b.gd should use tab indentation after fmt"
    );
}

#[test]
fn test_fmt_idempotent() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");
    let file_path = temp.path().join("idem.gd");

    // Unformatted input
    fs::write(
        &file_path,
        "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n",
    )
    .expect("write file");

    // First format
    gd_bin()
        .arg("fmt")
        .arg(&file_path)
        .output()
        .expect("first fmt");
    let after_first = fs::read_to_string(&file_path).unwrap();

    // Second format
    gd_bin()
        .arg("fmt")
        .arg(&file_path)
        .output()
        .expect("second fmt");
    let after_second = fs::read_to_string(&file_path).unwrap();

    assert_eq!(after_first, after_second, "Formatting should be idempotent");

    // --check should also pass now
    let check = gd_bin()
        .arg("fmt")
        .arg("--check")
        .arg(&file_path)
        .output()
        .expect("fmt --check");
    assert_eq!(
        check.status.code(),
        Some(0),
        "Already-formatted file should pass --check"
    );
}

#[test]
fn test_fmt_preserves_strings() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("strings.gd");

    let code = r#"extends Node


func _ready() -> void:
	var a: String = "hello world (with parens)"
	var b: String = "tabs\there\tand\tthere"
	var c: String = 'single "quotes" inside'
"#;
    fs::write(&file_path, code).expect("write file");

    let output = gd_bin()
        .arg("fmt")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt");

    assert!(output.status.success(), "gd fmt should succeed");

    let formatted = fs::read_to_string(&file_path).unwrap();
    assert!(
        formatted.contains(r#""hello world (with parens)""#),
        "String with parens should be preserved"
    );
    assert!(
        formatted.contains(r#""tabs\there\tand\tthere""#),
        "String with escapes should be preserved"
    );
    assert!(
        formatted.contains(r#"'single "quotes" inside'"#),
        "Single-quoted string should be preserved"
    );
}

#[test]
fn test_fmt_malformed_gdscript() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("broken.gd");

    // Intentionally broken GDScript
    fs::write(&file_path, "func (:\n\t\tif if if\n\t\t\t{\n").expect("write file");

    let output = gd_bin()
        .arg("fmt")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd fmt");

    // Should not crash (process didn't panic). Exit code may be non-zero, that's fine.
    assert!(
        output.status.code().is_some(),
        "gd fmt should not crash on malformed GDScript (should exit cleanly)"
    );
}
