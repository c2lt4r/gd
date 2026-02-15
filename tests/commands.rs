mod common;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

use common::{gd_bin, setup_gd_project};

// ─── new command ─────────────────────────────────────────────────────────────

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

// ─── completions command ─────────────────────────────────────────────────────

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

// ─── stats command ───────────────────────────────────────────────────────────

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
        "Stats output should contain 'Files:', got: {stdout}"
    );
    assert!(
        stdout.contains("Functions:"),
        "Stats output should contain 'Functions:', got: {stdout}"
    );
}

// ── stats --diff ─────────────────────────────────────────────────────────────

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

// ─── man command ─────────────────────────────────────────────────────────────

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
        "man page should contain roff .TH header, got: {stdout}"
    );
    assert!(
        stdout.contains("gd"),
        "man page should mention 'gd', got: {stdout}"
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
        "man page for fmt should contain 'fmt', got: {stdout}"
    );
}

// ─── config validation ───────────────────────────────────────────────────────

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
        "Should warn about unknown key in gd.toml, stderr: {stderr}"
    );
}

// ── CI auto-detect download URL ──────────────────────────────────────────────

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

// ── addons lock ──────────────────────────────────────────────────────────────

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

// ── eval command ─────────────────────────────────────────────────────────────

#[test]
fn test_eval_check_valid_expression() {
    let temp = setup_gd_project(&[]);

    let output = gd_bin()
        .args(["eval", "--check", "1 + 1"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd eval --check");

    // --check only validates the wrapper script parses; doesn't need Godot
    // It will fail at the Godot execution step (no Godot in CI), but the
    // pre-check itself should pass, so look for non-syntax errors
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("syntax error"),
        "Valid expression should pass --check, stderr: {stderr}"
    );
}

#[test]
fn test_eval_check_invalid_expression() {
    let temp = setup_gd_project(&[]);

    let output = gd_bin()
        .args(["eval", "--check", "if if if"])
        .current_dir(temp.path())
        .output()
        .expect("Failed to run gd eval --check");

    assert!(
        !output.status.success(),
        "Invalid expression should fail --check"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("syntax error"),
        "Should report syntax errors, stderr: {stderr}"
    );
}
