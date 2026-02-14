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
    // Subcommand groups
    assert!(stdout.contains("live"), "missing 'live' group");
    assert!(stdout.contains("scene"), "missing 'scene' group");
    assert!(stdout.contains("camera"), "missing 'camera' group");
    assert!(stdout.contains("select"), "missing 'select' group");
    // Flat commands still present
    assert!(stdout.contains("set-prop"));
    assert!(stdout.contains("suspend"));
    assert!(stdout.contains("next-frame"));
    assert!(stdout.contains("time-scale"));
    assert!(stdout.contains("reload-scripts"));
    assert!(stdout.contains("profiler"));
    // Old flat names should NOT appear (moved into groups)
    assert!(
        !stdout.contains("scene-tree"),
        "scene-tree should be under 'scene' group"
    );
    assert!(
        !stdout.contains("override-camera"),
        "override-camera should be under 'camera' group"
    );
    assert!(
        !stdout.contains("node-select-type"),
        "node-select-type should be under 'select' group"
    );
    assert!(
        !stdout.contains("live-set-root"),
        "live-set-root should be under 'live' group"
    );
    // DAP commands should NOT appear
    assert!(!stdout.contains("attach"));
    assert!(!stdout.contains("set-var"));
}

#[test]
fn test_debug_stepping_format_flag() {
    // Verify commands accept --format (including nested subcommands)
    let cmds: &[&[&str]] = &[
        &["debug", "scene", "tree", "--help"],
        &["debug", "next-frame", "--help"],
        &["debug", "reload-scripts", "--help"],
    ];
    for args in cmds {
        let output = gd_bin()
            .args(*args)
            .output()
            .unwrap_or_else(|_| panic!("failed to run gd {}", args.join(" ")));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let label = args.join(" ");
        assert!(output.status.success(), "gd {label} failed");
        assert!(
            stdout.contains("--format"),
            "gd {label} missing --format flag"
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

#[test]
fn test_debug_live_help() {
    let output = gd_bin()
        .args(["debug", "live", "--help"])
        .output()
        .expect("failed to run gd debug live --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("set-root"));
    assert!(stdout.contains("create-node"));
    assert!(stdout.contains("instantiate"));
    assert!(stdout.contains("remove-node"));
    assert!(stdout.contains("duplicate"));
    assert!(stdout.contains("reparent"));
    assert!(stdout.contains("node-prop"));
    assert!(stdout.contains("node-call"));
}

#[test]
fn test_debug_scene_help() {
    let output = gd_bin()
        .args(["debug", "scene", "--help"])
        .output()
        .expect("failed to run gd debug scene --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("tree"));
    assert!(stdout.contains("inspect"));
    assert!(stdout.contains("inspect-objects"));
    assert!(stdout.contains("camera-view"));
}

#[test]
fn test_debug_camera_help() {
    let output = gd_bin()
        .args(["debug", "camera", "--help"])
        .output()
        .expect("failed to run gd debug camera --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("override"));
    assert!(stdout.contains("transform-2d"));
    assert!(stdout.contains("transform-3d"));
    assert!(stdout.contains("screenshot"));
}

#[test]
fn test_debug_select_help() {
    let output = gd_bin()
        .args(["debug", "select", "--help"])
        .output()
        .expect("failed to run gd debug select --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("type"));
    assert!(stdout.contains("mode"));
    assert!(stdout.contains("visible"));
    assert!(stdout.contains("avoid-locked"));
    assert!(stdout.contains("prefer-group"));
    assert!(stdout.contains("reset-cam-2d"));
    assert!(stdout.contains("reset-cam-3d"));
    assert!(stdout.contains("clear"));
}
