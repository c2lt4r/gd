mod common;

use common::gd_bin;

#[test]
fn test_debug_help() {
    let output = gd_bin()
        .args(["debug", "--help"])
        .output()
        .expect("failed to run gd debug --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("attach"));
    assert!(stdout.contains("break"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("continue"));
    assert!(stdout.contains("next"));
    assert!(stdout.contains("step"));
    assert!(stdout.contains("pause"));
    assert!(stdout.contains("eval"));
    assert!(stdout.contains("set-var"));
}

#[test]
fn test_debug_set_var_help() {
    let output = gd_bin()
        .args(["debug", "set-var", "--help"])
        .output()
        .expect("failed to run gd debug set-var --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("--name"));
    assert!(stdout.contains("--value"));
    assert!(stdout.contains("--scope"));
}

#[test]
fn test_debug_break_help() {
    let output = gd_bin()
        .args(["debug", "break", "--help"])
        .output()
        .expect("failed to run gd debug break --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("--file"));
    assert!(stdout.contains("--line"));
    assert!(stdout.contains("--name"));
    assert!(stdout.contains("--condition"));
    assert!(stdout.contains("--timeout"));
    assert!(stdout.contains("--format"));
}

#[test]
fn test_debug_status_help() {
    let output = gd_bin()
        .args(["debug", "status", "--help"])
        .output()
        .expect("failed to run gd debug status --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("--format"));
}

#[test]
fn test_debug_no_connection() {
    // Run in a temp dir with no project.godot — daemon can't start
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let output = gd_bin()
        .args(["debug", "status"])
        .current_dir(tmp.path())
        .output()
        .expect("failed to run gd debug status");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not connect"),
        "expected connection error, got: {stderr}"
    );
}
