mod common;

use common::{gd_bin, setup_gd_project};

#[test]
fn test_daemon_help() {
    let output = gd_bin()
        .args(["daemon", "--help"])
        .output()
        .expect("failed to run gd daemon --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("status"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("restart"));
}

#[test]
fn test_hover_fallback_no_daemon() {
    let project = setup_gd_project(&[(
        "test.gd",
        "extends Node\n\nfunc hello():\n\tvar x = 42\n\tprint(x)\n",
    )]);

    // With --no-godot-proxy, should fall back to static analysis (no daemon needed)
    // Hover over the function name "hello" on line 3
    let output = gd_bin()
        .args([
            "query",
            "--no-godot-proxy",
            "hover",
            "--file",
            "test.gd",
            "--line",
            "3",
            "--column",
            "6",
        ])
        .current_dir(project.path())
        .output()
        .expect("failed to run hover");
    assert!(
        output.status.success(),
        "hover with --no-godot-proxy should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_completions_fallback_no_daemon() {
    let project = setup_gd_project(&[(
        "test.gd",
        "extends Node\n\nfunc hello():\n\tvar x = 42\n\tpr\n",
    )]);

    let output = gd_bin()
        .args([
            "query",
            "--no-godot-proxy",
            "completions",
            "--file",
            "test.gd",
            "--line",
            "5",
            "--column",
            "3",
        ])
        .current_dir(project.path())
        .output()
        .expect("failed to run completions");
    assert!(
        output.status.success(),
        "completions with --no-godot-proxy should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_definition_fallback_no_daemon() {
    let project = setup_gd_project(&[(
        "test.gd",
        "extends Node\n\nfunc hello():\n\tpass\n\nfunc caller():\n\thello()\n",
    )]);

    let output = gd_bin()
        .args([
            "query",
            "--no-godot-proxy",
            "definition",
            "--file",
            "test.gd",
            "--line",
            "7",
            "--column",
            "2",
        ])
        .current_dir(project.path())
        .output()
        .expect("failed to run definition");
    assert!(
        output.status.success(),
        "definition with --no-godot-proxy should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
