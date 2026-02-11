use std::io::{Read, Write};
use std::process::Command;
use std::fs;
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
    fs::write(&file_path, "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n")
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
    fs::write(&file_path, "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n")
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
    fs::write(&file_path, "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n")
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
        "extends Node\n\nsignal died\nsignal died\n\nfunc BadName():\n\tpass\n"
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
    fs::write(&file_path, "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n")
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
    fs::write(
        &file_path,
        "extends Node\n\nfunc BadName():\n\tpass\n"
    )
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
    fs::write(&file_path, "func BadName():\n\tpass\n")
        .expect("Failed to write file");

    let _output = gd_bin()
        .arg("lint")
        .arg("--fix")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --fix");

    // Read the file back
    let fixed_content = fs::read_to_string(&file_path)
        .expect("Failed to read fixed file");

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
        "[application]\nconfig/name=\"test\"\n"
    )
    .expect("Failed to write project.godot");

    fs::write(
        temp.path().join("test.gd"),
        "extends Node\n\nfunc _ready() -> void:\n\tpass\n"
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
        "func BadName():  # gd:ignore[naming-convention]\n\tpass\n"
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
        "# gd:ignore-next-line[naming-convention]\nfunc BadName():\n\tpass\n"
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

    fs::write(&file_path, "func BadName():\n\tpass\n")
        .expect("Failed to write file");

    let output = gd_bin()
        .arg("lint")
        .arg("--format")
        .arg("sarif")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --format sarif");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sarif: serde_json::Value = serde_json::from_str(&stdout)
        .expect("SARIF output should be valid JSON");

    assert_eq!(sarif["version"], "2.1.0", "SARIF version should be 2.1.0");
    assert!(
        sarif["runs"][0]["tool"]["driver"]["name"] == "gd",
        "SARIF tool name should be gd"
    );
    assert!(
        sarif["runs"][0]["results"].as_array().unwrap().len() > 0,
        "SARIF should contain results"
    );
}

#[test]
fn test_lsp_initialize() {
    use std::process::Stdio;

    fn lsp_msg(data: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(data).unwrap();
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
            .into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout.read_exact(&mut buf).expect("Failed to read header byte");
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

    assert!(output.status.success(), "gd fmt should succeed on directory");

    // Both files should now be formatted (tabs, not spaces)
    let a = fs::read_to_string(temp.path().join("a.gd")).unwrap();
    let b = fs::read_to_string(temp.path().join("b.gd")).unwrap();
    assert!(a.contains("\tpass"), "a.gd should use tab indentation after fmt");
    assert!(b.contains("\tpass"), "b.gd should use tab indentation after fmt");
}

#[test]
fn test_fmt_idempotent() {
    let temp = tempfile::Builder::new()
        .prefix("gdtest")
        .tempdir()
        .expect("Failed to create temp dir");
    let file_path = temp.path().join("idem.gd");

    // Unformatted input
    fs::write(&file_path, "extends Node\n\n\n\n\nfunc _ready()->void:\n  pass\n")
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
    assert_eq!(check.status.code(), Some(0), "Already-formatted file should pass --check");
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
    fs::write(&file_path, "func (:\n\t\tif if if\n\t\t\t{\n")
        .expect("write file");

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
    fs::write(
        addons.join("plugin.gd"),
        "func BadName():\n\tpass\n",
    )
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

    assert!(output.status.success(), "gd deps --format dot should succeed");

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

    assert!(output.status.success(), "gd deps --format json should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect("gd deps --format json should produce valid JSON");

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
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
            .into_bytes()
    }

    fn read_lsp_response(stdout: &mut impl Read) -> serde_json::Value {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            stdout.read_exact(&mut buf).expect("Failed to read header byte");
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
        assert!(edit.get("newText").is_some(), "TextEdit should have newText");
    }

    child.kill().ok();
}
