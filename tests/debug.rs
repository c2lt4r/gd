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
    // Use an unlikely port so DAP server won't be listening
    let output = gd_bin()
        .args(["debug", "--port", "19999", "status"])
        .output()
        .expect("failed to run gd debug status");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Could not connect"),
        "expected connection error, got: {stderr}"
    );
}
