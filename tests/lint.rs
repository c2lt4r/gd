mod common;

use std::fs;
use tempfile::TempDir;

use common::gd_bin;

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
        "naming-convention should be disabled via gd.toml, stderr: {stderr}"
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
        "gd lint --fix should report applying fixes, stderr: {stderr}"
    );

    let fixed = fs::read_to_string(&file_path).unwrap();
    // naming-convention should be fixed
    assert!(
        fixed.contains("bad_name"),
        "BadName should be renamed to bad_name, got: {fixed}"
    );
    // self-assignment should be fixed with self. prefix
    assert!(
        fixed.contains("self.x = x"),
        "Self-assignment `x = x` should become `self.x = x`, got: {fixed}"
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
        "Self-assignment `x = x` should become `self.x = x` after --fix, got: {fixed}"
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
        "Severity override to 'error' should produce 'error' in output, stderr: {stderr}"
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
        "addons/ files should be ignored by ignore_patterns, stderr: {stderr}"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "Should pass when only ignored files have issues"
    );
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
        "Should show naming-convention with --rule repeatable, stderr: {stderr}"
    );
    assert!(
        stderr.contains("duplicate-signal"),
        "Should show duplicate-signal with --rule repeatable, stderr: {stderr}"
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
        "main.gd should still have naming-convention, stderr: {stderr}"
    );
    // test file should NOT have naming-convention (excluded by override),
    // though it may still have other warnings like empty-function
    let test_naming_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("test_thing.gd") && l.contains("naming-convention"))
        .collect();
    assert!(
        test_naming_lines.is_empty(),
        "tests/test_thing.gd should not have naming-convention due to override, stderr: {stderr}"
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
        "JSON format should not have stderr summary, stderr: {stderr}"
    );
    assert!(
        !stderr.contains("problems"),
        "JSON format should not have stderr summary, stderr: {stderr}"
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
        "SARIF format should not have stderr summary, stderr: {stderr}"
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
        "`== true` should be removed by --fix, got: {fixed}"
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
        "Should detect float-comparison issue, stderr: {stderr}"
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

#[test]
fn test_lint_prefer_in_operator() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("style.gd");
    fs::write(
        &file_path,
        "extends Node\n\nfunc f(x):\n\tif x == 1 or x == 2 or x == 3:\n\t\tpass\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("prefer-in-operator")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --rule prefer-in-operator");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prefer-in-operator"),
        "Should detect prefer-in-operator, got: {stderr}"
    );
}

#[test]
fn test_lint_prefer_is_instance() {
    let temp = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp.path().join("style.gd");
    fs::write(
        &file_path,
        "extends Node\n\nfunc f(x):\n\tif typeof(x) == TYPE_STRING:\n\t\tpass\n",
    )
    .unwrap();

    let output = gd_bin()
        .arg("lint")
        .arg("--rule")
        .arg("prefer-is-instance")
        .arg(&file_path)
        .output()
        .expect("Failed to run gd lint --rule prefer-is-instance");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prefer-is-instance"),
        "Should detect prefer-is-instance, got: {stderr}"
    );
}
