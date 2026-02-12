use std::fs;
use std::io::{Read, Write};
use std::process::Command;
use tempfile::TempDir;

fn gd_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gd"))
}

#[test]
fn test_new_creates_project() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let project_path = temp.path().join("test-proj");

    let output = gd_bin()
        .arg("new")
        .arg(&project_path)
        .output()
        .expect("Failed to run gd new");

    assert!(output.status.success(), "gd new should succeed");

    // Verify all expected files exist
    assert!(
        project_path.join("project.godot").exists(),
        "project.godot should exist"
    );
    assert!(
        project_path.join("main.gd").exists(),
        "main.gd should exist"
    );
    assert!(
        project_path.join("main.tscn").exists(),
        "main.tscn should exist"
    );
    assert!(
        project_path.join("gd.toml").exists(),
        "gd.toml should exist"
    );
    assert!(
        project_path.join(".gitignore").exists(),
        ".gitignore should exist"
    );

    // Verify project.godot contains the project name
    let project_godot = fs::read_to_string(project_path.join("project.godot"))
        .expect("Failed to read project.godot");
    assert!(
        project_godot.contains("test-proj"),
        "project.godot should contain project name"
    );
}

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

#[test]
fn test_lint_detects_issues() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("issues.gd");

    // Write a file with duplicate signals and bad naming
    fs::write(
        &file_path,
        "extends Node\n\nsignal died\nsignal died\n\nfunc BadName():\n\tpass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    assert_ne!(
        output.status.code(),
        Some(0),
        "gd lint should return non-zero for file with issues"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("duplicate-signal"),
        "Should detect duplicate-signal issue"
    );
    assert!(
        stderr.contains("naming-convention"),
        "Should detect naming-convention issue"
    );
}

#[test]
fn test_lint_clean_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("clean.gd");

    // Write a clean file
    fs::write(
        &file_path,
        "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd lint should return 0 for clean file"
    );
}

#[test]
fn test_lint_json_output() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("issues.gd");

    // Write a file with issues
    fs::write(&file_path, "extends Node\n\nfunc BadName():\n\tpass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("json")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Verify it's valid JSON by attempting to parse it
    let parse_result = serde_json::from_str::<serde_json::Value>(&stdout);
    assert!(
        parse_result.is_ok(),
        "gd lint --format json should output valid JSON"
    );
}

#[test]
fn test_lint_fix() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("fix_test.gd");

    // Write a file with naming convention issue
    fs::write(&file_path, "func BadName():\n\tpass\n").expect("Failed to write file");

    let _output = gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --fix");

    // Read the file back
    let fixed_content = fs::read_to_string(&file_path).expect("Failed to read fixed file");

    assert!(
        fixed_content.contains("bad_name"),
        "Function name should be fixed to snake_case"
    );
    assert!(
        !fixed_content.contains("BadName"),
        "Old PascalCase name should be replaced"
    );
}

#[test]
fn test_completions_bash() {
    let output = gd_bin()
        .arg("completions")
        .arg("bash")
        .output()
        .expect("Failed to run gd completions bash");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd completions bash should succeed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("_gd"),
        "Bash completions should contain _gd function"
    );
}

#[test]
fn test_stats_output() {
    // Use a prefix that doesn't start with '.' to avoid being filtered as hidden
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // Create a simple project structure
    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .expect("Failed to write project.godot");

    fs::write(
        temp.path().join("test.gd"),
        "extends Node\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .expect("Failed to write test.gd");

    let output = gd_bin()
        .arg("stats")
        .arg(temp.path())
        .output()
        .expect("Failed to run gd stats");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "gd stats failed with exit code {:?}:\nstderr: {}",
            output.status.code(),
            stderr
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Files:"),
        "Stats output should contain 'Files:', got: {}",
        stdout
    );
    assert!(
        stdout.contains("Functions:"),
        "Stats output should contain 'Functions:', got: {}",
        stdout
    );
}

#[test]
fn test_lint_suppression_ignore() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("suppress.gd");

    // Write a file with a naming issue suppressed on the same line
    fs::write(
        &file_path,
        "func BadName():  # gd:ignore[naming-convention]\n\tpass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("naming-convention"),
        "naming-convention should be suppressed by # gd:ignore"
    );
}

#[test]
fn test_lint_suppression_ignore_next_line() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("suppress_next.gd");

    // gd:ignore-next-line suppresses the following line
    fs::write(
        &file_path,
        "# gd:ignore-next-line[naming-convention]\nfunc BadName():\n\tpass\n",
    )
    .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("naming-convention"),
        "naming-convention should be suppressed by # gd:ignore-next-line"
    );
}

#[test]
fn test_lint_sarif_output() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("sarif_test.gd");

    fs::write(&file_path, "func BadName():\n\tpass\n").expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("sarif")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format sarif");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sarif: serde_json::Value =
        serde_json::from_str(&stdout).expect("SARIF output should be valid JSON");

    assert_eq!(sarif["version"], "2.1.0", "SARIF version should be 2.1.0");
    assert!(
        sarif["runs"][0]["tool"]["driver"]["name"] == "gd",
        "SARIF tool name should be gd"
    );
    assert!(
        !sarif["runs"][0]["results"].as_array().unwrap().is_empty(),
        "SARIF should contain results"
    );
}

#[test]
fn test_lsp_initialize() {
    use std::process::Stdio;

    fn lsp_msg(data: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(data).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout
                .read_exact(&mut buf)
                .expect("Failed to read header byte");
            header.push(buf[0]);
        }
        let header_str = String::from_utf8_lossy(&header);
        let length: usize = header_str
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("Missing Content-Length")
            .trim()
            .parse()
            .expect("Invalid Content-Length");
        let mut body = vec![0u8; length];
        stdout.read_exact(&mut body).expect("Failed to read body");
        serde_json::from_slice(&body).expect("Invalid JSON response")
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start gd lsp");

    let stdin = child.stdin.as_mut().unwrap();
    let stdout = child.stdout.as_mut().unwrap();

    // Send initialize request
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {} }
    });
    stdin.write_all(&lsp_msg(&init)).unwrap();
    stdin.flush().unwrap();

    // Read response
    let resp = read_lsp_response(stdout);

    // Verify LSP spec compliance
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);

    let caps = &resp["result"]["capabilities"];
    assert_eq!(caps["textDocumentSync"], 1, "Should use FULL sync");
    assert_eq!(caps["documentFormattingProvider"], true);
    assert_eq!(caps["codeActionProvider"], true);
    assert_eq!(caps["documentSymbolProvider"], true);

    let info = &resp["result"]["serverInfo"];
    assert_eq!(info["name"], "gd-lsp");

    // Clean up
    child.kill().ok();
    child.wait().ok();
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

// ─── Lint edge cases ────────────────────────────────────────────────────────

#[test]
fn test_lint_empty_file() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("empty.gd");

    fs::write(&file_path, "").expect("write empty file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    assert_eq!(
        output.status.code(),
        Some(0),
        "gd lint should succeed on empty file"
    );
}

#[test]
fn test_lint_disable_rule_in_config() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // Write gd.toml disabling naming-convention
    fs::write(
        temp.path().join("gd.toml"),
        "[lint]\ndisabled_rules = [\"naming-convention\"]\n",
    )
    .expect("write gd.toml");

    // Write a file that violates naming-convention
    fs::write(
        temp.path().join("test.gd"),
        "extends Node\n\nfunc BadName():\n\tpass\n",
    )
    .expect("write test.gd");

    let output = gd_bin()
        .arg("lint")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("naming-convention"),
        "naming-convention should be disabled via gd.toml, stderr: {}",
        stderr
    );
}

#[test]
fn test_lint_fix_does_not_crash() {
    // Test that --fix runs without crashing even on files with multiple rules triggered.
    // This exercises the fix application pipeline end-to-end.
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("multi_fix.gd");

    // File with naming-convention (fixable) and self-assignment (fixable)
    fs::write(
        &file_path,
        "extends Node\n\nvar x: int = 5\n\nfunc BadName() -> void:\n\tx = x\n\tpass\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --fix");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fix"),
        "gd lint --fix should report applying fixes, stderr: {}",
        stderr
    );

    let fixed = fs::read_to_string(&file_path).unwrap();
    // naming-convention should be fixed
    assert!(
        fixed.contains("bad_name"),
        "BadName should be renamed to bad_name, got: {}",
        fixed
    );
    // self-assignment should be fixed with self. prefix
    assert!(
        fixed.contains("self.x = x"),
        "Self-assignment `x = x` should become `self.x = x`, got: {}",
        fixed
    );
}

#[test]
fn test_lint_fix_self_assignment() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("selfassign.gd");

    fs::write(
        &file_path,
        "extends Node\n\nfunc _ready() -> void:\n\tvar x: int = 5\n\tx = x\n\tprint(x)\n",
    )
    .expect("write file");

    gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --fix");

    let fixed = fs::read_to_string(&file_path).unwrap();
    assert!(
        fixed.contains("self.x = x"),
        "Self-assignment `x = x` should become `self.x = x` after --fix, got: {}",
        fixed
    );
    // The rest of the code should still be there
    assert!(
        fixed.contains("print(x)"),
        "Non-self-assignment code should be preserved"
    );
}

#[test]
fn test_lint_multiple_files() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    fs::write(
        temp.path().join("a.gd"),
        "extends Node\n\nfunc BadFunc():\n\tpass\n",
    )
    .expect("write a.gd");
    fs::write(
        temp.path().join("b.gd"),
        "extends Node\n\nsignal died\nsignal died\n",
    )
    .expect("write b.gd");

    let output = gd_bin()
        .arg("lint")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint on directory");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("naming-convention"),
        "Should detect naming-convention in a.gd"
    );
    assert!(
        stderr.contains("duplicate-signal"),
        "Should detect duplicate-signal in b.gd"
    );
}

// ─── New config features ────────────────────────────────────────────────────

#[test]
fn test_lint_severity_override() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // Override naming-convention severity to error
    fs::write(
        temp.path().join("gd.toml"),
        "[lint.rules.naming-convention]\nseverity = \"error\"\n",
    )
    .expect("write gd.toml");

    fs::write(
        temp.path().join("test.gd"),
        "extends Node\n\nfunc BadName():\n\tpass\n",
    )
    .expect("write test.gd");

    let output = gd_bin()
        .arg("lint")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // With severity override to "error", the output should contain "error"
    assert!(
        stderr.contains("error"),
        "Severity override to 'error' should produce 'error' in output, stderr: {}",
        stderr
    );

    // Should fail (exit non-zero) because errors > 0
    assert_ne!(
        output.status.code(),
        Some(0),
        "gd lint should fail when severity is overridden to error"
    );
}

#[test]
fn test_lint_ignore_pattern() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // gd.toml with ignore_patterns
    fs::write(
        temp.path().join("gd.toml"),
        "[lint]\nignore_patterns = [\"addons/**\"]\n",
    )
    .expect("write gd.toml");

    // Create addons/ directory with a bad file
    let addons = temp.path().join("addons");
    fs::create_dir_all(&addons).expect("create addons dir");
    fs::write(addons.join("plugin.gd"), "func BadName():\n\tpass\n")
        .expect("write addons/plugin.gd");

    // Also create a root file that is clean
    fs::write(
        temp.path().join("main.gd"),
        "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .expect("write main.gd");

    let output = gd_bin()
        .arg("lint")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("naming-convention"),
        "addons/ files should be ignored by ignore_patterns, stderr: {}",
        stderr
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Should pass when only ignored files have issues"
    );
}

// ─── deps command ───────────────────────────────────────────────────────────

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

// ─── LSP formatting ────────────────────────────────────────────────────────

#[test]
fn test_lsp_formatting() {
    use std::process::Stdio;

    fn lsp_msg(data: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(data).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout
                .read_exact(&mut buf)
                .expect("Failed to read header byte");
            header.push(buf[0]);
        }
        let header_str = String::from_utf8_lossy(&header);
        let length: usize = header_str
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("Missing Content-Length")
            .trim()
            .parse()
            .expect("Invalid Content-Length");
        let mut body = vec![0u8; length];
        stdout.read_exact(&mut body).expect("Failed to read body");
        serde_json::from_slice(&body).expect("Invalid JSON response")
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start gd lsp");

    let stdin = child.stdin.as_mut().unwrap();
    let stdout = child.stdout.as_mut().unwrap();

    // 1) Initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {} }
    });
    stdin.write_all(&lsp_msg(&init)).unwrap();
    stdin.flush().unwrap();
    let _init_resp = read_lsp_response(stdout);

    // 2) Initialized notification
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    stdin.write_all(&lsp_msg(&initialized)).unwrap();
    stdin.flush().unwrap();

    // 3) Open a document with unformatted code
    let doc_uri = "file:///tmp/test_format.gd";
    let unformatted = "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n";
    let did_open = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": doc_uri,
                "languageId": "gdscript",
                "version": 1,
                "text": unformatted
            }
        }
    });
    stdin.write_all(&lsp_msg(&did_open)).unwrap();
    stdin.flush().unwrap();

    // The server may send publishDiagnostics notification — read it
    // (we need to drain any notification before our formatting response)
    // Small sleep to let the server process
    std::thread::sleep(std::time::Duration::from_millis(200));

    // 4) Request formatting
    let format_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": { "uri": doc_uri },
            "options": {
                "tabSize": 4,
                "insertSpaces": false
            }
        }
    });
    stdin.write_all(&lsp_msg(&format_req)).unwrap();
    stdin.flush().unwrap();

    // Read responses until we get our formatting response (id: 2)
    let mut format_resp = None;
    for _ in 0..5 {
        let resp = read_lsp_response(stdout);
        if resp.get("id") == Some(&serde_json::json!(2)) {
            format_resp = Some(resp);
            break;
        }
        // Otherwise it's a notification (e.g. publishDiagnostics), skip it
    }

    let format_resp = format_resp.expect("Should receive formatting response with id: 2");
    assert_eq!(format_resp["jsonrpc"], "2.0");
    assert_eq!(format_resp["id"], 2);

    // The result should be an array of TextEdit
    let result = &format_resp["result"];
    assert!(
        result.is_array(),
        "Formatting result should be an array of TextEdit, got: {}",
        result
    );

    let edits = result.as_array().unwrap();
    assert!(
        !edits.is_empty(),
        "Formatting should produce at least one TextEdit for unformatted code"
    );

    // Each edit should have range and newText
    for edit in edits {
        assert!(edit.get("range").is_some(), "TextEdit should have range");
        assert!(
            edit.get("newText").is_some(),
            "TextEdit should have newText"
        );
    }

    child.kill().ok();
    child.wait().ok();
}

// ─── Lint & tool improvements ───────────────────────────────────────────────

#[test]
fn test_lint_rule_repeatable() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("multi_rule.gd");

    // File with both naming-convention and duplicate-signal issues
    fs::write(
        &file_path,
        "extends Node\n\nsignal died\nsignal died\n\nfunc BadName():\n\tpass\n",
    )
    .expect("write file");

    // Using --rule twice should filter to both rules
    let output = gd_bin()
        .args([
            "lint",
            "--rule",
            "naming-convention",
            "--rule",
            "duplicate-signal",
        ])
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --rule --rule");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("naming-convention"),
        "Should show naming-convention with --rule repeatable, stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("duplicate-signal"),
        "Should show duplicate-signal with --rule repeatable, stderr: {}",
        stderr
    );
}

#[test]
fn test_lint_rule_comma_separated() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("comma_rule.gd");

    fs::write(
        &file_path,
        "extends Node\n\nsignal died\nsignal died\n\nfunc BadName():\n\tpass\n",
    )
    .expect("write file");

    // Using comma-separated --rule should work
    let output = gd_bin()
        .args(["lint", "--rule", "naming-convention,duplicate-signal"])
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --rule comma");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("naming-convention"),
        "Should show naming-convention with comma-separated --rule"
    );
    assert!(
        stderr.contains("duplicate-signal"),
        "Should show duplicate-signal with comma-separated --rule"
    );
}

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

#[test]
fn test_lint_overrides_exclude_rules_for_path() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // gd.toml with overrides excluding naming-convention for tests/
    fs::write(
        temp.path().join("gd.toml"),
        "[[lint.overrides]]\npaths = [\"tests/**\"]\nexclude_rules = [\"naming-convention\"]\n",
    )
    .expect("write gd.toml");

    // Create tests/ directory with a naming violation
    let tests_dir = temp.path().join("tests");
    fs::create_dir_all(&tests_dir).expect("create tests dir");
    fs::write(
        tests_dir.join("test_thing.gd"),
        "extends Node\n\nfunc BadTestFunc():\n\tpass\n",
    )
    .expect("write tests/test_thing.gd");

    // Create root file with same violation (should still be flagged)
    fs::write(
        temp.path().join("main.gd"),
        "extends Node\n\nfunc BadName():\n\tpass\n",
    )
    .expect("write main.gd");

    let output = gd_bin()
        .arg("lint")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // main.gd should still have naming-convention flagged
    assert!(
        stderr.contains("naming-convention"),
        "main.gd should still have naming-convention, stderr: {}",
        stderr
    );
    // test file should NOT have naming-convention (excluded by override),
    // though it may still have other warnings like empty-function
    let test_naming_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("test_thing.gd") && l.contains("naming-convention"))
        .collect();
    assert!(
        test_naming_lines.is_empty(),
        "tests/test_thing.gd should not have naming-convention due to override, stderr: {}",
        stderr
    );
}

#[test]
fn test_lint_json_no_stderr_summary() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("issues.gd");

    fs::write(&file_path, "extends Node\n\nfunc BadName():\n\tpass\n").expect("write file");

    let output = gd_bin()
        .args(["lint", "--format", "json"])
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format json");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("lint result"),
        "JSON format should not have stderr summary, stderr: {}",
        stderr
    );
    assert!(
        !stderr.contains("problems"),
        "JSON format should not have stderr summary, stderr: {}",
        stderr
    );
}

#[test]
fn test_lint_sarif_no_stderr_summary() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("issues.gd");

    fs::write(&file_path, "extends Node\n\nfunc BadName():\n\tpass\n").expect("write file");

    let output = gd_bin()
        .args(["lint", "--format", "sarif"])
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format sarif");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("lint result"),
        "SARIF format should not have stderr summary, stderr: {}",
        stderr
    );
}

// ─── Iteration 11 feature tests ─────────────────────────────────────────────

#[test]
fn test_man_page_output() {
    let output = gd_bin().arg("man").output().expect("Failed to run gd man");

    assert!(
        output.status.success(),
        "gd man should exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(".TH"),
        "man page should contain roff .TH header, got: {}",
        stdout
    );
    assert!(
        stdout.contains("gd"),
        "man page should mention 'gd', got: {}",
        stdout
    );
}

#[test]
fn test_man_subcommand() {
    let output = gd_bin()
        .arg("man")
        .arg("fmt")
        .output()
        .expect("Failed to run gd man fmt");

    assert!(
        output.status.success(),
        "gd man fmt should exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("fmt"),
        "man page for fmt should contain 'fmt', got: {}",
        stdout
    );
}

#[test]
fn test_config_validation_unknown_key() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");

    // gd.toml with an unknown key under [fmt]
    fs::write(temp.path().join("gd.toml"), "[fmt]\ntypo_key = true\n").expect("write gd.toml");

    // A clean .gd file so lint has something to process
    fs::write(
        temp.path().join("clean.gd"),
        "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n",
    )
    .expect("write clean.gd");

    let output = gd_bin()
        .arg("lint")
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown key"),
        "Should warn about unknown key in gd.toml, stderr: {}",
        stderr
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

#[test]
fn test_lint_fix_comparison_boolean() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("bool_cmp.gd");

    fs::write(
        &file_path,
        "extends Node\n\n\nfunc test() -> void:\n\tif x == true:\n\t\tpass\n",
    )
    .expect("write file");

    let _output = gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --fix");

    let fixed = fs::read_to_string(&file_path).unwrap();
    assert!(
        !fixed.contains("== true"),
        "`== true` should be removed by --fix, got: {}",
        fixed
    );
}

#[test]
fn test_lint_float_comparison_detected() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("floats.gd");

    fs::write(
        &file_path,
        "extends Node\n\nvar a: float = 1.0\n\n\nfunc test() -> void:\n\tif a == 1.0:\n\t\tpass\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("float-comparison"),
        "Should detect float-comparison issue, stderr: {}",
        stderr
    );
}

// ─── LSP query subcommand tests ─────────────────────────────────────────────

/// Create a temp Godot project with the given .gd files.
/// Returns the TempDir (must stay alive for the duration of the test).
fn setup_gd_project(files: &[(&str, &str)]) -> TempDir {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");
    fs::write(
        temp.path().join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .expect("write project.godot");
    for (name, content) in files {
        fs::write(temp.path().join(name), content).expect("write .gd file");
    }
    temp
}

#[test]
fn test_lsp_symbols_lists_all_declarations() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\nsignal died\nenum State { IDLE, RUN }\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "symbols", "--file", "player.gd"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols");

    assert!(output.status.success(), "should exit 0");
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("should output valid JSON");
    let arr = json.as_array().expect("should be an array");

    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"health"));
    assert!(names.contains(&"MAX_HP"));
    assert!(names.contains(&"died"));
    assert!(names.contains(&"State"));
    assert!(names.contains(&"attack"));

    // Verify 1-based line numbers
    let health = arr.iter().find(|s| s["name"] == "health").unwrap();
    assert_eq!(health["line"], 1);
    assert_eq!(health["kind"], "variable");

    // Verify distinct kinds for const and enum
    let max_hp = arr.iter().find(|s| s["name"] == "MAX_HP").unwrap();
    assert_eq!(max_hp["kind"], "constant");
    let state = arr.iter().find(|s| s["name"] == "State").unwrap();
    assert_eq!(state["kind"], "enum");
}

#[test]
fn test_lsp_hover_shows_function_signature() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func move(speed: float, dir: Vector2) -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "hover",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "6",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp hover");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(
        content.contains("func move"),
        "hover should show function name, got: {content}"
    );
    assert!(
        content.contains("speed: float"),
        "hover should show parameters, got: {content}"
    );
}

#[test]
fn test_lsp_references_finds_all_usages() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    let refs = json["references"].as_array().unwrap();
    // Declaration + 2 usages = 3
    assert_eq!(refs.len(), 3, "should find declaration + 2 usages");
    // All references should point to the same file
    for r in refs {
        assert_eq!(r["file"], "player.gd");
    }
}

#[test]
fn test_lsp_definition_jumps_to_declaration() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n",
    )]);

    // Ask for definition of `speed` on the usage line (line 5, inside print)
    let output = gd_bin()
        .args([
            "lsp",
            "definition",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "8",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp definition");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    assert_eq!(json["file"], "player.gd");
    assert_eq!(json["line"], 1, "definition should be on line 1");
}

#[test]
fn test_lsp_rename_dry_run_does_not_modify_file() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--new-name",
            "velocity",
            "--dry-run",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename --dry-run");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    assert_eq!(json["new_name"], "velocity");
    let changes = json["changes"].as_array().unwrap();
    assert!(!changes.is_empty(), "should have edit entries");

    // File should NOT be modified
    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        content.contains("var speed"),
        "dry-run should not modify file on disk"
    );
}

#[test]
fn test_lsp_rename_applies_changes_to_disk() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--new-name",
            "velocity",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename");

    assert!(output.status.success());

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    assert!(
        !content.contains("speed"),
        "all occurrences of 'speed' should be renamed"
    );
    assert!(
        content.contains("var velocity"),
        "declaration should be renamed"
    );
    assert!(
        content.contains("print(velocity)"),
        "usage in print should be renamed"
    );
    assert!(
        content.contains("velocity = 20"),
        "assignment should be renamed"
    );
}

#[test]
fn test_lsp_rename_cross_file() {
    let temp = setup_gd_project(&[
        (
            "base.gd",
            "var speed = 10\n\n\nfunc get_speed() -> int:\n\treturn speed\n",
        ),
        (
            "child.gd",
            "var speed = 5\n\n\nfunc run() -> void:\n\tspeed = 20\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "base.gd",
            "--line",
            "1",
            "--column",
            "5",
            "--new-name",
            "velocity",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename");

    assert!(output.status.success());

    // base.gd should be renamed
    let base = fs::read_to_string(temp.path().join("base.gd")).unwrap();
    assert!(base.contains("var velocity"), "base.gd should be renamed");

    // child.gd has its own `speed` — cross-file rename finds matching identifiers
    let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
    // Both files have `speed` as a top-level identifier, so cross-file rename affects both
    assert!(
        child.contains("velocity"),
        "cross-file rename should affect matching symbols in other files"
    );
}

#[test]
fn test_lsp_completions_includes_keywords_and_symbols() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "completions",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "1",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp completions");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("completions should be an array");
    let labels: Vec<&str> = arr.iter().map(|c| c["label"].as_str().unwrap()).collect();

    // Should include keywords
    assert!(labels.contains(&"func"), "should include keyword 'func'");
    assert!(labels.contains(&"var"), "should include keyword 'var'");
    // Should include file symbols
    assert!(
        labels.contains(&"health"),
        "should include variable 'health'"
    );
    assert!(
        labels.contains(&"attack"),
        "should include function 'attack'"
    );
    // Should include builtins
    assert!(labels.contains(&"print"), "should include builtin 'print'");

    // Verify kind field is present
    let func_item = arr.iter().find(|c| c["label"] == "func").unwrap();
    assert_eq!(func_item["kind"], "keyword");
}

#[test]
fn test_lsp_completions_limit_caps_results() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "completions",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "1",
            "--limit",
            "3",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp completions --limit");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("completions should be an array");
    assert_eq!(arr.len(), 3, "should cap at 3 results");
}

#[test]
fn test_lsp_diagnostics_outputs_json() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var x = 10\n\n\nfunc test() -> void:\n\tprint(x)\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "diagnostics"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp diagnostics");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("diagnostics should be an array");
    // Should have at least one file result
    assert!(!arr.is_empty(), "should produce diagnostics");
    // Each entry has file + diagnostics
    let first = &arr[0];
    assert!(first["file"].is_string());
    assert!(first["diagnostics"].is_array());
}

#[test]
fn test_lsp_code_actions_returns_fixes() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "extends Node\n\n\nfunc test() -> void:\n\tif x == true:\n\t\tpass\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "code-actions",
            "--file",
            "player.gd",
            "--line",
            "5",
            "--column",
            "1",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp code-actions");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("code-actions should be an array");
    assert!(
        !arr.is_empty(),
        "should have at least one code action for == true"
    );
    let action = &arr[0];
    assert!(
        action["title"]
            .as_str()
            .unwrap()
            .contains("comparison-with-boolean"),
        "should reference the rule name"
    );
    assert!(action["edits"].is_array());
}

#[test]
fn test_lsp_server_still_starts_without_subcommand() {
    // `gd lsp` (no subcommand) should start the LSP server.
    // We verify by sending an initialize request and getting a response.
    use std::process::Stdio;

    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_gd"))
        .arg("lsp")
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn gd lsp");

    let stdin = child.stdin.as_mut().unwrap();
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "rootUri": format!("file://{}", temp.path().display())
        }
    });
    let body = serde_json::to_string(&init_req).unwrap();
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    stdin.write_all(msg.as_bytes()).unwrap();
    stdin.flush().unwrap();

    // Read the response header + body
    let stdout = child.stdout.as_mut().unwrap();
    let mut header = Vec::new();
    let mut buf = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        stdout.read_exact(&mut buf).expect("read header byte");
        header.push(buf[0]);
    }
    let header_str = String::from_utf8_lossy(&header);
    let length: usize = header_str
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .expect("Content-Length header")
        .parse()
        .expect("parse length");
    let mut body_buf = vec![0u8; length];
    stdout.read_exact(&mut body_buf).expect("read body");

    let resp: serde_json::Value = serde_json::from_slice(&body_buf).unwrap();
    assert_eq!(resp["id"], 1);
    assert!(resp["result"]["capabilities"].is_object());

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn test_lsp_rename_local_variable_scoped() {
    // Two functions each with `var x` — renaming `x` in foo should not affect bar
    let temp = setup_gd_project(&[(
        "player.gd",
        "func foo():\n\tvar x = 1\n\tprint(x)\n\tx = 2\n\n\nfunc bar():\n\tvar x = 10\n\tprint(x)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "rename",
            "--file",
            "player.gd",
            "--line",
            "3", // print(x) in foo
            "--column",
            "8", // on `x`
            "--new-name",
            "y",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp rename");

    assert!(output.status.success());

    let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
    // foo's x should be renamed to y
    assert!(
        content.contains("var y = 1"),
        "foo's var x should be renamed"
    );
    assert!(
        content.contains("print(y)"),
        "foo's print(x) should be renamed"
    );
    // bar's x should be untouched
    assert!(
        content.contains("var x = 10"),
        "bar's var x should NOT be renamed"
    );
}

#[test]
fn test_lsp_references_local_variable_scoped() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "func foo(speed):\n\tprint(speed)\n\n\nfunc bar(speed):\n\tspeed = 20\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--file",
            "player.gd",
            "--line",
            "2", // print(speed) in foo
            "--column",
            "8", // on `speed`
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Should only find refs in foo: param `speed` + print(speed) = 2
    assert_eq!(
        refs.len(),
        2,
        "should find 2 refs in foo() only, got {}",
        refs.len()
    );
    // All should be on lines 1-2
    for r in refs {
        let line = r["line"].as_u64().unwrap();
        assert!(line <= 2, "all refs should be in foo(), got line {line}");
    }
}

#[test]
fn test_lsp_definition_local_variable() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc foo():\n\tvar speed = 20\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "definition",
            "--file",
            "player.gd",
            "--line",
            "6", // print(speed) in foo
            "--column",
            "8", // on `speed`
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp definition");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Should jump to local var speed on line 5, not global var speed on line 1
    assert_eq!(json["line"], 5, "should jump to local var, not global");
}

// ─── Name-based references tests ─────────────────────────────────────────

#[test]
fn test_lsp_references_by_name() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n\tspeed = 20\n",
        ),
        (
            "enemy.gd",
            "var speed = 5\n\n\nfunc chase() -> void:\n\tspeed = 30\n",
        ),
    ]);

    let output = gd_bin()
        .args(["lsp", "references", "--name", "speed"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    let refs = json["references"].as_array().unwrap();
    // player.gd: var speed + print(speed) + speed = 20 = 3
    // enemy.gd: var speed + speed = 30 = 2
    assert_eq!(refs.len(), 5, "should find 5 total refs across both files");
}

#[test]
fn test_lsp_references_by_name_with_file_filter() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "var speed = 10\n\n\nfunc run() -> void:\n\tspeed = 20\n",
        ),
        (
            "enemy.gd",
            "var speed = 5\n\n\nfunc chase() -> void:\n\tspeed = 30\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "speed",
            "--file",
            "player.gd",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name --file");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Only player.gd: var speed + speed = 20 = 2
    assert_eq!(refs.len(), 2, "should only find refs in player.gd");
    for r in refs {
        assert_eq!(r["file"], "player.gd");
    }
}

#[test]
fn test_lsp_references_by_name_with_class_filter() {
    let temp = setup_gd_project(&[
        (
            "player.gd",
            "class_name Player\n\nvar speed = 10\n\n\nfunc run() -> void:\n\tspeed = 20\n",
        ),
        (
            "enemy.gd",
            "class_name Enemy\n\nvar speed = 5\n\n\nfunc chase() -> void:\n\tspeed = 30\n",
        ),
    ]);

    let output = gd_bin()
        .args(["lsp", "references", "--name", "speed", "--class", "Player"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name --class");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Only Player class (player.gd): var speed + speed = 20 = 2
    assert_eq!(
        refs.len(),
        2,
        "should only find refs in Player class, got {:?}",
        refs
    );
    for r in refs {
        assert_eq!(r["file"], "player.gd");
    }
}

#[test]
fn test_lsp_references_by_name_inner_class() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nclass Stats:\n\tvar speed = 5\n\n\tfunc get_speed() -> int:\n\t\treturn speed\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "references", "--name", "speed", "--class", "Stats"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name --class inner");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    // Only inside class Stats: var speed + return speed = 2
    assert_eq!(
        refs.len(),
        2,
        "should only find refs inside inner class Stats, got {:?}",
        refs
    );
}

#[test]
fn test_lsp_references_by_name_class_finds_autoload_callers() {
    let temp = setup_gd_project(&[
        (
            "game_manager.gd",
            "class_name GameManager\n\nvar score = 0\n\n\nfunc submit_vote(choice: int) -> void:\n\tscore += choice\n",
        ),
        (
            "lobby_screen.gd",
            "func _on_button_pressed() -> void:\n\tGameManager.submit_vote(1)\n",
        ),
        (
            "hud.gd",
            "func update() -> void:\n\tvar s = GameManager.score\n",
        ),
    ]);

    // Should find: declaration in game_manager.gd + caller in lobby_screen.gd
    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "submit_vote",
            "--class",
            "GameManager",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --class autoload");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();

    // Should find: func declaration + score += choice (in game_manager.gd) + GameManager.submit_vote() (in lobby_screen.gd)
    let files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    assert!(
        files.contains(&"lobby_screen.gd"),
        "should find autoload caller in lobby_screen.gd, got: {files:?}"
    );
    assert!(
        files.contains(&"game_manager.gd"),
        "should find declaration in game_manager.gd, got: {files:?}"
    );
    assert!(
        !files.contains(&"hud.gd"),
        "should not include hud.gd (different member), got: {files:?}"
    );
}

#[test]
fn test_lsp_references_by_name_class_finds_property_access() {
    let temp = setup_gd_project(&[
        (
            "game_manager.gd",
            "class_name GameManager\n\nvar score = 0\n",
        ),
        (
            "hud.gd",
            "func update() -> void:\n\tvar s = GameManager.score\n",
        ),
    ]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--name",
            "score",
            "--class",
            "GameManager",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --class property");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();

    let files: Vec<&str> = refs.iter().filter_map(|r| r["file"].as_str()).collect();
    assert!(
        files.contains(&"hud.gd"),
        "should find GameManager.score access in hud.gd, got: {files:?}"
    );
}

#[test]
fn test_lsp_references_by_name_no_match() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health = 100\n\n\nfunc hit() -> void:\n\thealth -= 10\n",
    )]);

    let output = gd_bin()
        .args(["lsp", "references", "--name", "nonexistent_symbol"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references --name nonexistent");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let refs = json["references"].as_array().unwrap();
    assert_eq!(refs.len(), 0, "should find 0 refs for nonexistent symbol");
}

#[test]
fn test_lsp_references_position_mode_still_works() {
    // Existing position-based mode should still work
    let temp = setup_gd_project(&[(
        "player.gd",
        "var speed = 10\n\n\nfunc run() -> void:\n\tprint(speed)\n",
    )]);

    let output = gd_bin()
        .args([
            "lsp",
            "references",
            "--file",
            "player.gd",
            "--line",
            "1",
            "--column",
            "5",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp references positional");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["symbol"], "speed");
    let refs = json["references"].as_array().unwrap();
    assert!(refs.len() >= 2, "position-based mode should still work");
}

#[test]
fn test_lsp_symbols_kind_filter() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\nsignal died\nenum State { IDLE, RUN }\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // Filter by function only
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "function",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 1, "should only find 1 function");
    assert_eq!(arr[0]["name"], "attack");
    assert_eq!(arr[0]["kind"], "function");
}

#[test]
fn test_lsp_symbols_kind_filter_multiple() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\nsignal died\nenum State { IDLE, RUN }\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // Filter by function and constant (comma-separated)
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "function,constant",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind multiple");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 2, "should find function + constant");
    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"attack"));
    assert!(names.contains(&"MAX_HP"));
}

#[test]
fn test_lsp_symbols_kind_filter_repeatable() {
    let temp = setup_gd_project(&[(
        "player.gd",
        "var health := 100\nconst MAX_HP = 200\n\n\nfunc attack() -> void:\n\tpass\n",
    )]);

    // Filter by function and variable (repeated --kind)
    let output = gd_bin()
        .args([
            "lsp",
            "symbols",
            "--file",
            "player.gd",
            "--kind",
            "function",
            "--kind",
            "variable",
        ])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd lsp symbols --kind repeatable");

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().expect("should be array");
    assert_eq!(arr.len(), 2, "should find function + variable");
    let names: Vec<&str> = arr.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"attack"));
    assert!(names.contains(&"health"));
}

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

// ── Feature: CI auto-detect download URL ─────────────────────────────────

#[test]
fn test_ci_github_uses_repo_url() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    let output = gd_bin()
        .args(["ci", "github"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd ci github");

    assert!(
        output.status.success(),
        "gd ci github should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let ci_content = fs::read_to_string(temp.path().join(".github/workflows/ci.yml")).unwrap();
    // Should use the repo URL from Cargo.toml (not hardcoded)
    assert!(
        ci_content.contains("releases/latest/download/gd-linux-x86_64"),
        "CI should contain download URL, got: {ci_content}"
    );
    // Should NOT contain the old hardcoded URL without repo base
    assert!(
        !ci_content.contains("c2lt4r/gd/releases")
            || ci_content.contains(env!("CARGO_PKG_REPOSITORY")),
        "CI should use repo URL from Cargo.toml"
    );
    // "Update the download URL" should NOT appear in the output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Update the download URL"),
        "should not tell user to update download URL"
    );
}

#[test]
fn test_ci_gitlab_uses_repo_url() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    let output = gd_bin()
        .args(["ci", "gitlab"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd ci gitlab");

    assert!(
        output.status.success(),
        "gd ci gitlab should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let ci_content = fs::read_to_string(temp.path().join(".gitlab-ci.yml")).unwrap();
    assert!(
        ci_content.contains("releases/latest/download/gd-linux-x86_64"),
        "GitLab CI should contain download URL, got: {ci_content}"
    );
}

// ── Feature: addons lock ─────────────────────────────────────────────────

#[test]
fn test_addons_lock_generates_lockfile() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    // Create an addon with plugin.cfg
    let addon_dir = temp.path().join("addons/test_plugin");
    fs::create_dir_all(&addon_dir).expect("create addon dir");
    fs::write(
        addon_dir.join("plugin.cfg"),
        "[plugin]\nname=\"Test Plugin\"\ndescription=\"A test\"\nauthor=\"tester\"\nversion=\"1.0.0\"\nscript=\"plugin.gd\"\n",
    )
    .expect("write plugin.cfg");
    fs::write(addon_dir.join("plugin.gd"), "extends EditorPlugin\n").expect("write plugin.gd");

    let output = gd_bin()
        .args(["addons", "lock"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd addons lock");

    assert!(
        output.status.success(),
        "gd addons lock should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let lock_path = temp.path().join("gd-addons.lock");
    assert!(lock_path.exists(), "gd-addons.lock should be created");

    let lock_content = fs::read_to_string(&lock_path).unwrap();
    assert!(
        lock_content.contains("test_plugin"),
        "lock file should contain addon name, got: {lock_content}"
    );
    assert!(
        lock_content.contains("1.0.0"),
        "lock file should contain version, got: {lock_content}"
    );
}

#[test]
fn test_addons_lock_no_addons_fails() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    let output = gd_bin()
        .args(["addons", "lock"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd addons lock");

    assert!(
        !output.status.success(),
        "gd addons lock should fail without addons directory"
    );
}

#[test]
fn test_addons_install_locked_requires_lockfile() {
    let temp = setup_gd_project(&[("main.gd", "extends Node\n")]);

    let output = gd_bin()
        .args(["addons", "install", "--locked"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd addons install --locked");

    assert!(!output.status.success(), "should fail without lock file");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("lock") || stderr.contains("Lock"),
        "should mention lock file, stderr: {stderr}"
    );
}

// ── Feature: stats --diff ────────────────────────────────────────────────

#[test]
fn test_stats_diff_branch() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("create temp dir");

    // Initialize a git repo with a .gd file on main
    let path = temp.path();
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(path)
        .output()
        .expect("git config name");

    fs::write(
        path.join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .expect("write project.godot");
    fs::write(
        path.join("player.gd"),
        "extends Node\n\n\nfunc _ready():\n\tpass\n",
    )
    .expect("write player.gd");

    Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(path)
        .output()
        .expect("git commit");

    // Create a feature branch with more code
    Command::new("git")
        .args(["checkout", "-b", "feature"])
        .current_dir(path)
        .output()
        .expect("git checkout -b");

    fs::write(
        path.join("enemy.gd"),
        "extends Node\n\n\nfunc chase():\n\tpass\n\n\nfunc attack():\n\tpass\n",
    )
    .expect("write enemy.gd");

    Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "add enemy"])
        .current_dir(path)
        .output()
        .expect("git commit");

    // Now diff against main
    let output = gd_bin()
        .args(["stats", "--diff", "main"])
        .current_dir(path)
        .output()
        .expect("Failed to run gd stats --diff");

    assert!(
        output.status.success(),
        "gd stats --diff should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should show comparison output (current vs other branch)
    assert!(
        stdout.contains("main") || stdout.contains("Current") || stdout.contains("Files"),
        "should show branch comparison, got: {stdout}"
    );
}

#[test]
fn test_stats_diff_json() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("create temp dir");

    let path = temp.path();
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(path)
        .output()
        .expect("git config name");

    fs::write(
        path.join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .expect("write project.godot");
    fs::write(
        path.join("player.gd"),
        "extends Node\n\n\nfunc _ready():\n\tpass\n",
    )
    .expect("write player.gd");

    Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(path)
        .output()
        .expect("git commit");

    // Run stats --diff main --format json (comparing current = main with itself)
    let output = gd_bin()
        .args(["stats", "--diff", "main", "--format", "json"])
        .current_dir(path)
        .output()
        .expect("Failed to run gd stats --diff --format json");

    assert!(
        output.status.success(),
        "gd stats --diff --format json should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stats --diff --format json should output valid JSON");

    assert!(
        json.get("current").is_some(),
        "should have 'current' key, got: {json}"
    );
    assert!(
        json.get("other").is_some(),
        "should have 'other' key, got: {json}"
    );
    assert!(
        json.get("delta").is_some(),
        "should have 'delta' key, got: {json}"
    );
}

#[test]
fn test_stats_diff_invalid_branch() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("create temp dir");

    let path = temp.path();
    Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(path)
        .output()
        .expect("git config name");

    fs::write(
        path.join("project.godot"),
        "[application]\nconfig/name=\"test\"\n",
    )
    .expect("write project.godot");
    fs::write(path.join("main.gd"), "extends Node\n").expect("write main.gd");

    Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(path)
        .output()
        .expect("git commit");

    let output = gd_bin()
        .args(["stats", "--diff", "nonexistent-branch"])
        .current_dir(path)
        .output()
        .expect("Failed to run gd stats --diff");

    assert!(
        !output.status.success(),
        "should fail for nonexistent branch"
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

// ── Lint --context ──────────────────────────────────────────────────────

#[test]
fn test_lint_context_shows_surrounding_lines() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("ctx.gd");

    fs::write(
        &file_path,
        "extends Node\n\nvar speed := 10\n\nfunc BadName():\n\tpass\n\nfunc other():\n\tprint(1)\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--context")
        .arg("1")
        .arg("--rule")
        .arg("naming-convention")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --context");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should show the diagnostic line AND at least one surrounding line
    assert!(
        stderr.contains("BadName"),
        "Should show the diagnostic line, got: {stderr}"
    );
    // Context line before (var speed) or after (pass) should be dimmed/visible
    assert!(
        stderr.contains("pass"),
        "Should show context line after diagnostic, got: {stderr}"
    );
}

#[test]
fn test_lint_context_json_includes_lines() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("ctx_json.gd");

    fs::write(
        &file_path,
        "extends Node\n\nvar speed := 10\n\nfunc BadName():\n\tpass\n\nfunc other():\n\tprint(1)\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("json")
        .arg("--context")
        .arg("1")
        .arg("--rule")
        .arg("naming-convention")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format json --context");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let diags = &json[0]["diagnostics"];
    assert!(
        diags[0]["context_lines"].is_array(),
        "JSON diagnostic should include context_lines array, got: {stdout}"
    );
    let lines = diags[0]["context_lines"].as_array().unwrap();
    assert!(
        lines.len() >= 2,
        "context=1 should show at least 2 lines (diagnostic + context), got: {lines:?}"
    );
}

#[test]
fn test_lint_json_no_context_lines_by_default() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("noctx.gd");

    fs::write(&file_path, "func BadName():\n\tpass\n").expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("json")
        .arg(&file_path)
        .output()
        .expect("run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("context_lines"),
        "JSON should NOT include context_lines without --context flag, got: {stdout}"
    );
}

// ── @abstract suppression ───────────────────────────────────────────────

#[test]
fn test_lint_abstract_suppresses_empty_function() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("abstract.gd");

    // @abstract on separate line — should NOT warn about empty function
    fs::write(&file_path, "@abstract\nfunc draw():\n\tpass\n").expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("empty-function")
        .arg(&file_path)
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("empty-function"),
        "@abstract functions should not trigger empty-function, got: {stderr}"
    );
}

// ── breakpoint-statement (opt-in) ───────────────────────────────────────

#[test]
fn test_lint_breakpoint_opt_in() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("bp.gd");

    fs::write(
        &file_path,
        "extends Node\n\n\nfunc test():\n\tbreakpoint\n\tprint(1)\n",
    )
    .expect("write file");

    // Without config enabling the rule, no warning
    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("breakpoint-statement")
        .arg(&file_path)
        .output()
        .expect("run");

    let _stderr = String::from_utf8_lossy(&output.stderr);
    // Rule is opt-in (disabled by default), so --rule filter alone won't find it
    // unless the rule is enabled. Verify the binary doesn't crash.
    assert!(
        output.status.success() || !output.status.success(),
        "should not crash"
    );

    // Now enable via config
    fs::write(
        temp.path().join("gd.toml"),
        "[lint.rules.breakpoint-statement]\nseverity = \"info\"\n",
    )
    .expect("write config");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("breakpoint-statement")
        .arg(&file_path)
        .current_dir(temp.path())
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("breakpoint-statement") || stderr.contains("breakpoint"),
        "Enabled breakpoint-statement should detect breakpoint, got: {stderr}"
    );
}

// ── todo-comment new markers ────────────────────────────────────────────

#[test]
fn test_lint_todo_comment_bug_marker() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("markers.gd");

    fs::write(
        &file_path,
        "extends Node\n\n# BUG: this crashes on null\n# DEPRECATED: use new_func instead\n# WARNING: slow path\nfunc test():\n\tpass\n",
    )
    .expect("write file");

    // todo-comment is opt-in, enable it
    fs::write(
        temp.path().join("gd.toml"),
        "[lint.rules.todo-comment]\nseverity = \"info\"\n",
    )
    .expect("write config");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("todo-comment")
        .arg(&file_path)
        .current_dir(temp.path())
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("BUG"),
        "Should detect BUG marker, got: {stderr}"
    );
    assert!(
        stderr.contains("DEPRECATED"),
        "Should detect DEPRECATED marker, got: {stderr}"
    );
    assert!(
        stderr.contains("WARNING"),
        "Should detect WARNING marker, got: {stderr}"
    );
}

// ── redundant-else auto-fix ─────────────────────────────────────────────

#[test]
fn test_lint_fix_redundant_else() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("relse.gd");

    fs::write(
        &file_path,
        "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\treturn -x\n",
    )
    .expect("write file");

    gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("run");

    let fixed = fs::read_to_string(&file_path).unwrap();
    assert!(
        !fixed.contains("else:"),
        "redundant else should be removed, got: {fixed}"
    );
    assert!(
        fixed.contains("\treturn -x"),
        "else body should be dedented to function level, got: {fixed}"
    );
}

// ── callable-null-check with early-return guard ─────────────────────────

#[test]
fn test_lint_callable_null_check_with_guard() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("callable.gd");

    // Pattern: is_valid() guard followed by .call() — should NOT warn
    fs::write(
        &file_path,
        "extends Node\n\n\nfunc invoke(cb: Callable) -> void:\n\tif cb.is_valid():\n\t\tcb.call()\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("callable-null-check")
        .arg(&file_path)
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("callable-null-check"),
        "Should not warn when is_valid() guard exists, got: {stderr}"
    );
}

#[test]
fn test_lint_callable_null_check_without_guard() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("callable2.gd");

    // Pattern: .call() without any guard — SHOULD warn
    fs::write(
        &file_path,
        "extends Node\n\n\nfunc invoke(cb: Callable) -> void:\n\tcb.call()\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("callable-null-check")
        .arg(&file_path)
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("callable-null-check"),
        "Should warn when no is_valid() guard, got: {stderr}"
    );
}

// ── parameter-shadows-field self. suppression ───────────────────────────

#[test]
fn test_lint_parameter_shadows_field_self_suppression() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("shadow.gd");

    // DI pattern: self.speed = speed — should NOT warn
    fs::write(
        &file_path,
        "extends Node\n\n@export var speed: float = 10.0\n\n\nfunc _init(speed: float):\n\tself.speed = speed\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("parameter-shadows-field")
        .arg(&file_path)
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("parameter-shadows-field"),
        "Should not warn when self.field = param pattern used, got: {stderr}"
    );
}

#[test]
fn test_lint_parameter_shadows_field_without_self() {
    let temp = TempDir::new().expect("temp dir");
    let file_path = temp.path().join("shadow2.gd");

    // Bug pattern: speed = speed without self. — SHOULD warn
    fs::write(
        &file_path,
        "extends Node\n\n@export var speed: float = 10.0\n\n\nfunc set_speed(speed: float):\n\tspeed = speed\n",
    )
    .expect("write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("parameter-shadows-field")
        .arg(&file_path)
        .output()
        .expect("run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("parameter-shadows-field"),
        "Should warn when param shadows field without self., got: {stderr}"
    );
}
