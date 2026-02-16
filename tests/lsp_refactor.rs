mod common;

use std::fs;
use std::io::Write;
use std::process::Command;

use common::{gd_bin, setup_gd_project};

// ─── Refactoring command tests ──────────────────────────────────────────────

#[test]
fn test_lsp_delete_symbol_function() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health = 100\n\n\nfunc unused():\n\tpass\n\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "player.gd",
            "--name",
            "unused",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "unused");
    assert_eq!(json["kind"], "function");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(!content.contains("unused"), "function should be removed");
    assert!(content.contains("health"), "other symbols should remain");
    assert!(content.contains("_ready"), "other functions should remain");
}

#[test]
fn test_lsp_delete_symbol_dry_run() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func unused():\n\tpass\n\n\nfunc keep():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "player.gd",
            "--name",
            "unused",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("unused"), "dry-run should not modify file");
}

#[test]
fn test_lsp_delete_symbol_with_references() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "player.gd",
            "--name",
            "speed",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol");

    // Should exit non-zero when references exist
    assert!(
        !output.status.success(),
        "should fail when references exist"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);
    assert!(
        !json["references"].as_array().unwrap().is_empty(),
        "should list references"
    );
}

#[test]
fn test_lsp_delete_symbol_force() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "player.gd",
            "--name",
            "speed",
            "--force",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol --force");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(!content.contains("var speed"), "should be deleted");
}

#[test]
fn test_lsp_delete_symbol_by_line() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var a = 1\n\n\nfunc target():\n\tpass\n\n\nfunc keep():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "delete-symbol", "--file", "player.gd", "--line", "4"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol --line");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "target");
    assert_eq!(json["applied"], true);
}

#[test]
fn test_lsp_move_symbol_to_new_file() {
    let temp = setup_gd_project(&[("source.gd", "var keep = 1\n\n\nfunc helper():\n\tpass\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "helper",
            "--from",
            "source.gd",
            "--to",
            "helpers.gd",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "helper");
    assert_eq!(json["applied"], true);

    assert!(
        temp.path().join("helpers.gd").exists(),
        "target file should be created"
    );
    let target = fs::read_to_string(temp.path().join("helpers.gd")).unwrap();
    assert!(target.contains("func helper()"));
    let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
    assert!(!source.contains("helper"));
    assert!(source.contains("keep"));
}

#[test]
fn test_lsp_move_symbol_to_existing_file() {
    let temp = setup_gd_project(&[
        (
            "source.gd",
            "func to_move():\n\tpass\n\n\nfunc stay():\n\tpass\n",
        ),
        ("target.gd", "func existing():\n\tpass\n"),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "to_move",
            "--from",
            "source.gd",
            "--to",
            "target.gd",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol to existing");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
    assert!(target.contains("func existing()"));
    assert!(target.contains("func to_move()"));
}

#[test]
fn test_lsp_move_symbol_dry_run() {
    let temp = setup_gd_project(&[(
        "source.gd",
        "func helper():\n\tpass\n\n\nfunc keep():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "helper",
            "--from",
            "source.gd",
            "--to",
            "target.gd",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    assert!(
        !temp.path().join("target.gd").exists(),
        "dry-run should not create file"
    );
    let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
    assert!(
        source.contains("helper"),
        "dry-run should not modify source"
    );
}

#[test]
fn test_lsp_extract_method_simple() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func _ready():\n\tvar x = 1\n\tprint(x)\n\tprint(\"done\")\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "4",
            "--end-line",
            "4",
            "--name",
            "do_print",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method");

    assert!(
        output.status.success(),
        "extract-method should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert!(json["function"].as_str().unwrap().contains("func do_print"));

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func do_print():"),
        "new function should be created, got: {content}"
    );
    assert!(
        content.contains("do_print()"),
        "call site should exist, got: {content}"
    );
}

#[test]
fn test_lsp_extract_method_with_params() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process():\n\tvar health = 100\n\tvar armor = 50\n\tprint(health)\n\tprint(armor)\n\tprint(\"end\")\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "4",
            "--end-line",
            "5",
            "--name",
            "show_stats",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method with params");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    let params = json["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 2, "should capture 2 parameters");
}

#[test]
fn test_lsp_extract_method_with_return() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process():\n\tvar health = 100\n\thealth -= 10\n\tprint(health)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "3",
            "--end-line",
            "3",
            "--name",
            "take_damage",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method with return");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["returns"], "health");

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("health = take_damage(health)"),
        "call site should assign return, got: {content}"
    );
    assert!(
        content.contains("return health"),
        "function should return, got: {content}"
    );
}

#[test]
fn test_lsp_extract_method_dry_run() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func _ready():\n\tprint(\"hello\")\n\tprint(\"world\")\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--name",
            "greet",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("func greet"),
        "dry-run should not modify file"
    );
}

// ──────────────────────────────────────────────
// Feature 5: Async extraction detection
// ──────────────────────────────────────────────

#[test]
fn test_lsp_extract_method_async_warning() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process():\n\tawait get_tree().create_timer(1.0).timeout\n\tprint(\"done\")\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--name",
            "wait_a_bit",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    let warnings = json["warnings"].as_array().unwrap();
    assert!(
        !warnings.is_empty(),
        "should warn about await in extracted code"
    );
    assert!(warnings[0].as_str().unwrap().contains("await"));
}

#[test]
fn test_lsp_extract_method_no_async_no_warning() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process():\n\tprint(\"hello\")\n\tprint(\"world\")\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--name",
            "greet",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // warnings should be absent or empty
    assert!(
        json.get("warnings").is_none() || json["warnings"].as_array().unwrap().is_empty(),
        "no async warnings expected"
    );
}

// ──────────────────────────────────────────────
// Feature 2: Multiple return values
// ──────────────────────────────────────────────

#[test]
fn test_lsp_extract_method_multi_return() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process():\n\tvar x = 1\n\tvar y = 2\n\tx += 1\n\ty += 1\n\tprint(x)\n\tprint(y)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-method",
            "--file",
            "player.gd",
            "--start-line",
            "4",
            "--end-line",
            "5",
            "--name",
            "increment",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-method multi-return");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    let return_vars = json["return_vars"].as_array().unwrap();
    assert_eq!(return_vars.len(), 2, "should have 2 return vars");

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("Dictionary"),
        "should use Dictionary return type, got: {content}"
    );
    assert!(
        content.contains("_result"),
        "should have _result variable, got: {content}"
    );
}

// ──────────────────────────────────────────────
// Feature 3: Inner class member operations
// ──────────────────────────────────────────────

#[test]
fn test_lsp_delete_symbol_inner_class() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "class Inner:\n\tvar keep = 1\n\tfunc unused():\n\t\tpass\n\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "player.gd",
            "--name",
            "unused",
            "--class",
            "Inner",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol --class");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "unused");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("unused"),
        "inner class function should be removed"
    );
    assert!(content.contains("keep"), "other members should remain");
    assert!(content.contains("_ready"), "top-level should remain");
}

#[test]
fn test_lsp_move_symbol_with_class() {
    let temp = setup_gd_project(&[
        (
            "source.gd",
            "class Src:\n\tvar keep = 1\n\tfunc helper():\n\t\tpass\n",
        ),
        ("target.gd", "func existing():\n\tpass\n"),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "helper",
            "--from",
            "source.gd",
            "--to",
            "target.gd",
            "--class",
            "Src",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol --class");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
    assert!(
        target.contains("func helper()"),
        "target should have the function, got: {target}"
    );
    let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
    assert!(
        !source.contains("helper"),
        "source class should no longer have helper"
    );
    assert!(source.contains("keep"), "other members should remain");
}

// ──────────────────────────────────────────────
// Feature 4: Enum member delete
// ──────────────────────────────────────────────

#[test]
fn test_lsp_delete_enum_member() {
    let temp = setup_gd_project(&[(
        "state.gd",
        "enum State { IDLE, RUNNING, JUMPING }\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "state.gd",
            "--name",
            "State.RUNNING",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol enum member");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["kind"], "enum_member");

    let content = fs::read_to_string(temp.path().join("state.gd")).unwrap();
    assert!(
        !content.contains("RUNNING"),
        "RUNNING should be removed, got: {content}"
    );
    assert!(content.contains("IDLE"), "IDLE should remain");
    assert!(content.contains("JUMPING"), "JUMPING should remain");
}

#[test]
fn test_lsp_delete_enum_member_dry_run() {
    let temp = setup_gd_project(&[("state.gd", "enum State { IDLE, RUNNING }\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "delete-symbol",
            "--file",
            "state.gd",
            "--name",
            "State.IDLE",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp delete-symbol enum member --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("state.gd")).unwrap();
    assert!(content.contains("IDLE"), "dry-run should not modify file");
}

// ──────────────────────────────────────────────
// Feature 1: Preload path detection on move
// ──────────────────────────────────────────────

#[test]
fn test_lsp_move_symbol_reports_preloads() {
    let temp = setup_gd_project(&[
        ("source.gd", "var keep = 1\n\n\nfunc helper():\n\tpass\n"),
        (
            "user.gd",
            "var s = preload(\"res://source.gd\")\n\nfunc _ready():\n\tpass\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "helper",
            "--from",
            "source.gd",
            "--to",
            "target.gd",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    // Should report preloads referencing the source file
    let preloads = json["preloads"].as_array();
    assert!(
        preloads.is_some() && !preloads.unwrap().is_empty(),
        "should report preload references to source file, got: {json}"
    );
}

// ──────────────────────────────────────────────
// Feature 8: Self-reference warnings on move
// ──────────────────────────────────────────────

#[test]
fn test_lsp_move_symbol_self_ref_warning() {
    let temp = setup_gd_project(&[
        (
            "source.gd",
            "class Src:\n\tvar health = 100\n\tfunc take_damage():\n\t\tself.health -= 10\n",
        ),
        ("target.gd", "class Dst:\n\tvar armor = 50\n"),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "take_damage",
            "--from",
            "source.gd",
            "--to",
            "target.gd",
            "--class",
            "Src",
            "--target-class",
            "Dst",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol with self-ref");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    // Should warn about self.health not existing in Dst
    let warnings = json["warnings"].as_array();
    assert!(
        warnings.is_some() && !warnings.unwrap().is_empty(),
        "should warn about missing self.health in target class, got: {json}"
    );
    assert!(
        warnings.unwrap()[0].as_str().unwrap().contains("health"),
        "warning should mention 'health'"
    );
}

// ──────────────────────────────────────────────
// Feature 6: Inline method
// ──────────────────────────────────────────────

#[test]
fn test_lsp_inline_method_simple() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func greet():\n\tprint(\"hello\")\n\n\nfunc _ready():\n\tgreet()\n\tprint(\"done\")\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "inline-method",
            "--file",
            "player.gd",
            "--line",
            "6",
            "--column",
            "2",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp inline-method");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["function"], "greet");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("print(\"hello\")"),
        "inlined body should be present, got: {content}"
    );
}

#[test]
fn test_lsp_inline_method_with_params() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func double(x):\n\treturn x * 2\n\n\nfunc _ready():\n\tvar result = double(5)\n\tprint(result)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "inline-method",
            "--file",
            "player.gd",
            "--line",
            "6",
            "--column",
            "16",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp inline-method with params");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["function"], "double");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("5 * 2"),
        "should substitute params, got: {content}"
    );
}

// ──────────────────────────────────────────────
// Feature 7: Change function signature
// ──────────────────────────────────────────────

#[test]
fn test_lsp_change_signature_add_param() {
    let temp = setup_gd_project(&[
        ("player.gd", "func attack(target):\n\tprint(target)\n"),
        (
            "main.gd",
            "var p = preload(\"res://player.gd\")\n\nfunc _ready():\n\tp.attack(\"enemy\")\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "change-signature",
            "--file",
            "player.gd",
            "--name",
            "attack",
            "--add-param",
            "damage: int = 10",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp change-signature");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["function"], "attack");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("damage"),
        "new param should be in definition, got: {content}"
    );
    assert!(
        content.contains("int"),
        "type hint should be present, got: {content}"
    );
}

#[test]
fn test_lsp_change_signature_remove_param() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func attack(target, damage):\n\tprint(target)\n\n\nfunc _ready():\n\tattack(\"enemy\", 10)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "change-signature",
            "--file",
            "player.gd",
            "--name",
            "attack",
            "--remove-param",
            "damage",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp change-signature remove");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func attack(target)"),
        "damage param should be removed from definition, got: {content}"
    );
    assert!(
        content.contains("attack(\"enemy\")"),
        "damage arg should be removed from call site, got: {content}"
    );
}

#[test]
fn test_lsp_change_signature_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "func attack(target):\n\tprint(target)\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "change-signature",
            "--file",
            "player.gd",
            "--name",
            "attack",
            "--add-param",
            "damage",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp change-signature --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("damage"),
        "dry-run should not modify file"
    );
}

// ── Feature: introduce-variable ──────────────────────────────────────────

#[test]
fn test_lsp_introduce_variable_simple() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process(delta):\n\tposition.x += speed * delta\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "introduce-variable",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "17",
            "--end-column",
            "29",
            "--name",
            "velocity",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp introduce-variable");

    assert!(
        output.status.success(),
        "introduce-variable should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["variable"], "velocity");
    assert_eq!(json["expression"], "speed * delta");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("var velocity = speed * delta"),
        "should insert variable declaration, got: {content}"
    );
    assert!(
        content.contains("position.x += velocity"),
        "should replace expression with variable name, got: {content}"
    );
}

#[test]
fn test_lsp_introduce_variable_dry_run() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func process(delta):\n\tposition.x += speed * delta\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "introduce-variable",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "17",
            "--end-column",
            "29",
            "--name",
            "velocity",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp introduce-variable --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("var velocity"),
        "dry-run should not modify file"
    );
}

#[test]
fn test_lsp_introduce_variable_call_expression() {
    let temp = setup_gd_project(&[("player.gd", "func _ready():\n\tprint(get_health())\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "introduce-variable",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "8",
            "--end-column",
            "20",
            "--name",
            "hp",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp introduce-variable call");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["expression"], "get_health()");

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("var hp = get_health()"),
        "should extract call, got: {content}"
    );
    assert!(
        content.contains("print(hp)"),
        "should replace with var, got: {content}"
    );
}

// ── Feature: introduce-parameter ─────────────────────────────────────────

#[test]
fn test_lsp_introduce_parameter_with_type() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func move(delta):\n\tposition.x += 100.0 * delta\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "introduce-parameter",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "16",
            "--end-column",
            "21",
            "--name",
            "speed",
            "--type",
            "float",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp introduce-parameter");

    assert!(
        output.status.success(),
        "introduce-parameter should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["expression"], "100.0");
    assert_eq!(json["function"], "move");
    assert_eq!(json["applied"], true);
    assert!(
        json["parameter"]
            .as_str()
            .unwrap()
            .contains("speed: float = 100.0"),
        "parameter should have type hint"
    );

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func move(delta, speed: float = 100.0)"),
        "should add typed parameter, got: {content}"
    );
    assert!(
        content.contains("position.x += speed * delta"),
        "should replace literal with param name, got: {content}"
    );
}

#[test]
fn test_lsp_introduce_parameter_no_type() {
    let temp = setup_gd_project(&[("player.gd", "func greet():\n\tprint(\"hello\")\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "introduce-parameter",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "8",
            "--end-column",
            "15",
            "--name",
            "msg",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp introduce-parameter no type");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func greet(msg = \"hello\")"),
        "should add untyped param with default, got: {content}"
    );
    assert!(
        content.contains("print(msg)"),
        "should replace expression, got: {content}"
    );
}

#[test]
fn test_lsp_introduce_parameter_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "func greet():\n\tprint(\"hello\")\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "introduce-parameter",
            "--file",
            "player.gd",
            "--line",
            "2",
            "--column",
            "8",
            "--end-column",
            "15",
            "--name",
            "msg",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp introduce-parameter --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(!content.contains("msg"), "dry-run should not modify file");
}

// ── bulk-delete-symbol ──────────────────────────────────────────────────────

#[test]
fn test_lsp_bulk_delete_symbol() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var a = 1\nvar b = 2\nvar c = 3\n\n\nfunc keep():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "bulk-delete-symbol",
            "--file",
            "player.gd",
            "--names",
            "a,b,c",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp bulk-delete-symbol");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["deleted"].as_array().unwrap().len(), 3);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(!content.contains("var a"));
    assert!(!content.contains("var b"));
    assert!(!content.contains("var c"));
    assert!(content.contains("func keep()"));
}

#[test]
fn test_lsp_bulk_delete_symbol_skips_referenced() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\nvar unused = 0\n\n\nfunc run():\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "bulk-delete-symbol",
            "--file",
            "player.gd",
            "--names",
            "speed,unused",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp bulk-delete-symbol");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["deleted"].as_array().unwrap().len(), 1);
    assert_eq!(json["deleted"][0]["name"], "unused");
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"][0]["name"], "speed");
}

#[test]
fn test_lsp_bulk_delete_symbol_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "var a = 1\nvar b = 2\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "bulk-delete-symbol",
            "--file",
            "player.gd",
            "--names",
            "a,b",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp bulk-delete-symbol --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);
    assert_eq!(json["deleted"].as_array().unwrap().len(), 2);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("var a"), "dry-run should not modify");
    assert!(content.contains("var b"), "dry-run should not modify");
}

// ── bulk-rename ─────────────────────────────────────────────────────────────

#[test]
fn test_lsp_bulk_rename() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\nvar health = 100\n\n\nfunc _ready():\n\tprint(speed)\n\tprint(health)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "bulk-rename",
            "--file",
            "player.gd",
            "--renames",
            "speed:velocity,health:hp",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp bulk-rename");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["renames"].as_array().unwrap().len(), 2);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("velocity"), "speed should be renamed");
    assert!(content.contains("hp"), "health should be renamed");
    assert!(!content.contains("var speed"), "old name should be gone");
    assert!(!content.contains("var health"), "old name should be gone");
}

#[test]
fn test_lsp_bulk_rename_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "var speed = 10\nvar health = 100\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "bulk-rename",
            "--file",
            "player.gd",
            "--renames",
            "speed:velocity,health:hp",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp bulk-rename --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);
    assert_eq!(json["renames"].as_array().unwrap().len(), 2);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("speed"), "dry-run should not modify");
    assert!(content.contains("health"), "dry-run should not modify");
}

#[test]
fn test_lsp_bulk_rename_some_not_found() {
    let temp = setup_gd_project(&[("player.gd", "var speed = 10\n")]);

    let output = gd_bin()
        .args([
            "lsp",
            "bulk-rename",
            "--file",
            "player.gd",
            "--renames",
            "speed:velocity,nonexistent:whatever",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp bulk-rename");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["renames"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"][0]["old_name"], "nonexistent");
}

// ── inline-delegate ─────────────────────────────────────────────────────────

#[test]
fn test_lsp_inline_delegate() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var weapon = null\n\n\nfunc attack():\n\tweapon.fire()\n\n\nfunc _ready():\n\tattack()\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "inline-delegate",
            "--file",
            "player.gd",
            "--name",
            "attack",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp inline-delegate");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["delegate_target"], "weapon.fire");
    assert_eq!(json["function_deleted"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("weapon.fire()"),
        "caller should be replaced with delegate"
    );
    assert!(
        !content.contains("func attack()"),
        "delegate function should be deleted"
    );
}

#[test]
fn test_lsp_inline_delegate_dry_run() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var w = null\n\n\nfunc attack():\n\tw.fire()\n\n\nfunc _ready():\n\tattack()\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "inline-delegate",
            "--file",
            "player.gd",
            "--name",
            "attack",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp inline-delegate --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func attack()"),
        "dry-run should not modify"
    );
}

#[test]
fn test_lsp_inline_delegate_not_delegate() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func foo():\n\tprint(1)\n\tbar()\n\n\nfunc _ready():\n\tfoo()\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "inline-delegate",
            "--file",
            "player.gd",
            "--name",
            "foo",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp inline-delegate");

    assert!(
        !output.status.success(),
        "multi-statement function should not be a delegate"
    );
}

// ── extract-class ───────────────────────────────────────────────────────────

#[test]
fn test_lsp_extract_class() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc helper():\n\tpass\n\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-class",
            "--file",
            "player.gd",
            "--symbols",
            "helper",
            "--to",
            "helpers.gd",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-class");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["extracted"].as_array().unwrap().len(), 1);

    let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !source.contains("func helper()"),
        "symbol should be removed from source"
    );
    assert!(source.contains("_ready"), "other symbols should remain");

    let dest = fs::read_to_string(temp.path().join("helpers.gd")).unwrap();
    assert!(
        dest.contains("func helper()"),
        "symbol should appear in destination"
    );
}

#[test]
fn test_lsp_extract_class_dry_run() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc helper():\n\tpass\n\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-class",
            "--file",
            "player.gd",
            "--symbols",
            "helper",
            "--to",
            "helpers.gd",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-class --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func helper()"),
        "dry-run should not modify"
    );
    assert!(
        !temp.path().join("helpers.gd").exists(),
        "dry-run should not create file"
    );
}

#[test]
fn test_lsp_extract_class_multiple_symbols() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\nvar health = 100\n\n\nfunc helper():\n\tpass\n\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "extract-class",
            "--file",
            "player.gd",
            "--symbols",
            "speed,helper",
            "--to",
            "extracted.gd",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp extract-class");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["extracted"].as_array().unwrap().len(), 2);

    let source = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(!source.contains("var speed"));
    assert!(!source.contains("func helper()"));
    assert!(source.contains("health"), "non-extracted should remain");

    let dest = fs::read_to_string(temp.path().join("extracted.gd")).unwrap();
    assert!(dest.contains("var speed"));
    assert!(dest.contains("func helper()"));
}

// ── move-symbol --update-callers ────────────────────────────────────────────

#[test]
fn test_lsp_move_symbol_update_callers() {
    let temp = setup_gd_project(&[
        ("source.gd", "func helper():\n\tpass\n"),
        (
            "caller.gd",
            "const Source = preload(\"res://source.gd\")\n\n\nfunc _ready():\n\tSource.helper()\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "move-symbol",
            "--name",
            "helper",
            "--from",
            "source.gd",
            "--to",
            "dest.gd",
            "--update-callers",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp move-symbol --update-callers");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);

    // The helper should be in dest.gd
    let dest = fs::read_to_string(temp.path().join("dest.gd")).unwrap();
    assert!(dest.contains("func helper()"), "symbol should be in dest");

    // caller.gd should have a new preload for dest.gd
    let caller = fs::read_to_string(temp.path().join("caller.gd")).unwrap();
    assert!(
        caller.contains("res://dest.gd"),
        "caller should have preload to dest, got: {caller}"
    );
}

// ── AST-aware edit commands ─────────────────────────────────────────────────

/// Helper: run a gd lsp subcommand with stdin content piped in.
fn run_lsp_edit(dir: &std::path::Path, args: &[&str], stdin_content: &str) -> std::process::Output {
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn gd lsp");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_content.as_bytes())
        .unwrap();
    child.wait_with_output().expect("Failed to wait on child")
}

#[test]
fn test_lsp_replace_body() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-body",
            "--file",
            "player.gd",
            "--name",
            "_ready",
            "--no-format",
        ],
        "\tprint(\"hello\")\n",
    );

    assert!(output.status.success(), "replace-body should succeed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "replace-body");
    assert_eq!(json["symbol"], "_ready");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("print(\"hello\")"));
    assert!(!content.contains("\tpass"));
    assert!(content.contains("func _ready():"));
}

#[test]
fn test_lsp_replace_body_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-body",
            "--file",
            "player.gd",
            "--name",
            "_ready",
            "--no-format",
            "--dry-run",
        ],
        "\tprint(\"hello\")\n",
    );

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("\tpass"), "dry-run should not modify file");
}

#[test]
fn test_lsp_replace_body_reindents() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    // Send content with no indentation — should be reindented to 1 tab
    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-body",
            "--file",
            "player.gd",
            "--name",
            "_ready",
            "--no-format",
        ],
        "print(\"a\")\nprint(\"b\")\n",
    );

    assert!(output.status.success());
    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("\tprint(\"a\")"));
    assert!(content.contains("\tprint(\"b\")"));
}

#[test]
fn test_lsp_replace_body_non_function_fails() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\nvar speed = 10\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-body",
            "--file",
            "player.gd",
            "--name",
            "speed",
            "--no-format",
        ],
        "\t42\n",
    );

    assert!(!output.status.success(), "should fail for non-function");
}

#[test]
fn test_lsp_replace_body_with_class() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "extends Node\n\n\nclass Inner:\n\tfunc foo():\n\t\tpass\n",
    )]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-body",
            "--file",
            "player.gd",
            "--name",
            "foo",
            "--class",
            "Inner",
            "--no-format",
        ],
        "\t\tprint(1)\n",
    );

    assert!(output.status.success());
    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("\t\tprint(1)"));
}

#[test]
fn test_lsp_insert_after() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "insert",
            "--file",
            "player.gd",
            "--after",
            "_ready",
            "--no-format",
        ],
        "\nfunc _process(delta):\n\tpass\n",
    );

    assert!(output.status.success(), "insert --after should succeed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "insert");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("func _process(delta):"));
    let ready_pos = content.find("func _ready()").unwrap();
    let process_pos = content.find("func _process(delta)").unwrap();
    assert!(process_pos > ready_pos);
}

#[test]
fn test_lsp_insert_before() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "insert",
            "--file",
            "player.gd",
            "--before",
            "_ready",
            "--no-format",
        ],
        "var speed = 10\n",
    );

    assert!(output.status.success(), "insert --before should succeed");
    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("var speed = 10"));
    let var_pos = content.find("var speed").unwrap();
    let ready_pos = content.find("func _ready()").unwrap();
    assert!(var_pos < ready_pos);
}

#[test]
fn test_lsp_insert_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "insert",
            "--file",
            "player.gd",
            "--after",
            "_ready",
            "--no-format",
            "--dry-run",
        ],
        "\nfunc foo():\n\tpass\n",
    );

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(!content.contains("func foo()"));
}

#[test]
fn test_lsp_insert_no_anchor_fails() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &["lsp", "insert", "--file", "player.gd", "--no-format"],
        "var x = 1\n",
    );

    assert!(
        !output.status.success(),
        "should fail without --after or --before"
    );
}

#[test]
fn test_lsp_insert_input_file() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    // Write content to a temp file instead of piping through stdin
    let input_path = temp.path().join("_input.tmp");
    fs::write(&input_path, "\nfunc _process(delta):\n\tpass\n").unwrap();

    let output = gd_bin()
        .args([
            "lsp",
            "insert",
            "--file",
            "player.gd",
            "--after",
            "_ready",
            "--no-format",
            "--input-file",
        ])
        .arg(&input_path)
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp insert --input-file");

    assert!(
        output.status.success(),
        "insert --input-file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "insert");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("func _process(delta):"),
        "inserted content should appear in file"
    );
}

#[test]
fn test_lsp_replace_symbol() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "extends Node\nvar speed = 10\n\n\nfunc _ready():\n\tpass\n",
    )]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-symbol",
            "--file",
            "player.gd",
            "--name",
            "speed",
            "--no-format",
        ],
        "var speed: float = 42.0\n",
    );

    assert!(output.status.success(), "replace-symbol should succeed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "replace-symbol");
    assert_eq!(json["symbol"], "speed");
    assert_eq!(json["applied"], true);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("var speed: float = 42.0"));
    assert!(!content.contains("var speed = 10"));
}

#[test]
fn test_lsp_replace_symbol_function() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "extends Node\n\n\nfunc old_func():\n\tvar x = 1\n\tprint(x)\n",
    )]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-symbol",
            "--file",
            "player.gd",
            "--name",
            "old_func",
            "--no-format",
        ],
        "func new_func():\n\tprint(\"replaced\")\n",
    );

    assert!(output.status.success());
    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("func new_func():"));
    assert!(content.contains("print(\"replaced\")"));
    assert!(!content.contains("old_func"));
}

#[test]
fn test_lsp_replace_symbol_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\nvar speed = 10\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-symbol",
            "--file",
            "player.gd",
            "--name",
            "speed",
            "--no-format",
            "--dry-run",
        ],
        "var speed = 99\n",
    );

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("var speed = 10"));
}

#[test]
fn test_lsp_edit_range() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "extends Node\nvar a = 1\nvar b = 2\nvar c = 3\n",
    )]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "edit-range",
            "--file",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "3",
            "--no-format",
        ],
        "var x = 10\nvar y = 20\n",
    );

    assert!(output.status.success(), "edit-range should succeed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "edit-range");
    assert_eq!(json["applied"], true);
    assert!(json.get("symbol").is_none() || json["symbol"].is_null());

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("var x = 10"));
    assert!(content.contains("var y = 20"));
    assert!(!content.contains("var a = 1"));
    assert!(!content.contains("var b = 2"));
    assert!(content.contains("var c = 3"));
}

#[test]
fn test_lsp_edit_range_dry_run() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\nvar a = 1\nvar b = 2\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "edit-range",
            "--file",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--no-format",
            "--dry-run",
        ],
        "var replaced = 99\n",
    );

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("var a = 1"));
}

#[test]
fn test_lsp_edit_range_invalid_lines() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\nvar a = 1\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "edit-range",
            "--file",
            "player.gd",
            "--start-line",
            "5",
            "--end-line",
            "3",
            "--no-format",
        ],
        "x\n",
    );

    assert!(!output.status.success(), "should fail with invalid range");
}

#[test]
fn test_lsp_replace_body_with_format() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    // Don't pass --no-format — formatter should run
    let output = run_lsp_edit(
        temp.path(),
        &[
            "lsp",
            "replace-body",
            "--file",
            "player.gd",
            "--name",
            "_ready",
        ],
        "\tprint( \"hello\" )\n",
    );

    assert!(
        output.status.success(),
        "replace-body with format should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(content.contains("print("), "result should contain print");
    assert!(content.contains("func _ready():"), "signature preserved");
}

#[test]
fn test_lsp_create_file_stdin() {
    let temp = setup_gd_project(&[]);

    let custom_script =
        "extends CharacterBody2D\n\n\nfunc _physics_process(delta):\n\tmove_and_slide()\n";

    let output = run_lsp_edit(
        temp.path(),
        &["lsp", "create-file", "--file", "player.gd"],
        custom_script,
    );

    assert!(
        output.status.success(),
        "create-file with stdin should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], true);
    assert_eq!(json["file"], "player.gd");

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("extends CharacterBody2D"),
        "should use stdin content, not boilerplate"
    );
    assert!(
        content.contains("move_and_slide"),
        "should contain stdin content"
    );
    assert!(
        !content.contains("func _ready"),
        "should not contain boilerplate"
    );
}
