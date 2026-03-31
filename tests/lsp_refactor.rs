mod common;

use std::fs;
use std::io::Write;
use std::process::Command;

use common::{gd_bin, setup_gd_project};

// ─── Refactoring command tests ──────────────────────────────────────────────

#[test]
fn test_lsp_extract_method_simple() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func _ready():\n\tvar x = 1\n\tprint(x)\n\tprint(\"done\")\n",
    )]);

    let output = gd_bin()
        .args([
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "4",
            "--end-line",
            "4",
            "--name",
            "do_print",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method");

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
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "4",
            "--end-line",
            "5",
            "--name",
            "show_stats",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method with params");

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
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "3",
            "--end-line",
            "3",
            "--name",
            "take_damage",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method with return");

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
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--name",
            "greet",
            "--dry-run",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method --dry-run");

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
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--name",
            "wait_a_bit",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method");

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
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "2",
            "--end-line",
            "2",
            "--name",
            "greet",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method");

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
            "refactor",
            "extract-method",
            "player.gd",
            "--start-line",
            "4",
            "--end-line",
            "5",
            "--name",
            "increment",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor extract-method multi-return");

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
            "refactor",
            "change-signature",
            "player.gd",
            "--name",
            "attack",
            "--add-param",
            "damage: int = 10",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor change-signature");

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
            "refactor",
            "change-signature",
            "player.gd",
            "--name",
            "attack",
            "--remove-param",
            "damage",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor change-signature remove");

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
            "refactor",
            "change-signature",
            "player.gd",
            "--name",
            "attack",
            "--add-param",
            "damage",
            "--dry-run",
            "--format",
            "json",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd refactor change-signature --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["applied"], false);

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("damage"),
        "dry-run should not modify file"
    );
}

// ── AST-aware edit commands ─────────────────────────────────────────────────

/// Helper: run a gd edit subcommand with stdin content piped in.
fn run_lsp_edit(dir: &std::path::Path, args: &[&str], stdin_content: &str) -> std::process::Output {
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn gd edit");
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
            "edit",
            "replace-body",
            "player.gd",
            "--name",
            "_ready",
            "--no-format",
            "--format",
            "json",
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
fn test_lsp_replace_body_reindents() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    // Send content with no indentation — should be reindented to 1 tab
    let output = run_lsp_edit(
        temp.path(),
        &[
            "edit",
            "replace-body",
            "player.gd",
            "--name",
            "_ready",
            "--no-format",
            "--format",
            "json",
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
            "edit",
            "replace-body",
            "player.gd",
            "--name",
            "speed",
            "--no-format",
            "--format",
            "json",
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
            "edit",
            "replace-body",
            "player.gd",
            "--name",
            "foo",
            "--class",
            "Inner",
            "--no-format",
            "--format",
            "json",
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
            "edit",
            "insert",
            "player.gd",
            "--after",
            "_ready",
            "--no-format",
            "--format",
            "json",
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
            "edit",
            "insert",
            "player.gd",
            "--before",
            "_ready",
            "--no-format",
            "--format",
            "json",
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
fn test_lsp_insert_no_anchor_fails() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    let output = run_lsp_edit(
        temp.path(),
        &[
            "edit",
            "insert",
            "player.gd",
            "--no-format",
            "--format",
            "json",
        ],
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
            "edit",
            "insert",
            "player.gd",
            "--after",
            "_ready",
            "--no-format",
            "--input-file",
        ])
        .arg(&input_path)
        .args(["--format", "json"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd edit insert --input-file");

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
            "edit",
            "replace-symbol",
            "player.gd",
            "--name",
            "speed",
            "--no-format",
            "--format",
            "json",
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
            "edit",
            "replace-symbol",
            "player.gd",
            "--name",
            "old_func",
            "--no-format",
            "--format",
            "json",
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
fn test_lsp_replace_body_with_format() {
    let temp = setup_gd_project(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);

    // Don't pass --no-format — formatter should run
    let output = run_lsp_edit(
        temp.path(),
        &[
            "edit",
            "replace-body",
            "player.gd",
            "--name",
            "_ready",
            "--format",
            "json",
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
        &["edit", "create-file", "player.gd", "--format", "json"],
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

#[test]
fn test_lsp_create_file_stdin_with_class_name() {
    let temp = setup_gd_project(&[]);

    let body = "## Deserializes input packets.\n\n\nfunc parse(buf: PackedByteArray) -> Dictionary:\n\treturn {}\n";

    let output = run_lsp_edit(
        temp.path(),
        &[
            "edit",
            "create-file",
            "input_deserializer.gd",
            "--class-name",
            "InputDeserializer",
            "--extends",
            "RefCounted",
            "--format",
            "json",
        ],
        body,
    );

    assert!(
        output.status.success(),
        "create-file with stdin + --class-name should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(temp.path().join("input_deserializer.gd")).unwrap();
    assert!(
        content.contains("class_name InputDeserializer"),
        "should have class_name header: {content}"
    );
    assert!(
        content.contains("extends RefCounted"),
        "should have extends header: {content}"
    );
    assert!(
        content.contains("## Deserializes input packets."),
        "should contain body content: {content}"
    );
    // class_name should come before extends
    let cn_pos = content.find("class_name").unwrap();
    let ext_pos = content.find("extends").unwrap();
    assert!(
        cn_pos < ext_pos,
        "class_name should appear before extends: {content}"
    );
}
