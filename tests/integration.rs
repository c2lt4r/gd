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
    // self-assignment should be removed
    assert!(
        !fixed.contains("x = x"),
        "Self-assignment should be removed, got: {}",
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
        !fixed.contains("x = x"),
        "Self-assignment `x = x` should be removed by --fix, got: {}",
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
