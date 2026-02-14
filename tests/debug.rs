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
    // Binary debug protocol commands
    assert!(stdout.contains("scene-tree"));
    assert!(stdout.contains("inspect"));
    assert!(stdout.contains("set-prop"));
    assert!(stdout.contains("suspend"));
    assert!(stdout.contains("next-frame"));
    assert!(stdout.contains("time-scale"));
    assert!(stdout.contains("reload-scripts"));
    assert!(stdout.contains("override-camera"));
    assert!(stdout.contains("save-node"));
    assert!(stdout.contains("profiler"));
    // DAP commands should NOT appear
    assert!(!stdout.contains("attach"));
    assert!(!stdout.contains("set-var"));
}

#[test]
fn test_debug_stepping_format_flag() {
    // Verify binary protocol commands accept --format
    for cmd in ["scene-tree", "next-frame", "reload-scripts"] {
        let output = gd_bin()
            .args(["debug", cmd, "--help"])
            .output()
            .unwrap_or_else(|_| panic!("failed to run gd debug {cmd} --help"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "gd debug {cmd} --help failed");
        assert!(
            stdout.contains("--format"),
            "gd debug {cmd} missing --format flag"
        );
    }
}

#[test]
fn test_debug_server_help() {
    let output = gd_bin()
        .args(["debug", "server", "--help"])
        .output()
        .expect("failed to run gd debug server --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("--port"));
    assert!(stdout.contains("--wait"));
    assert!(stdout.contains("--timeout"));
}
