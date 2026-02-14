#![allow(dead_code)]

use std::fs;
use std::process::Command;

use tempfile::TempDir;

pub fn gd_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gd"))
}

/// Create a temp Godot project with the given .gd files.
/// Returns the `TempDir` (must stay alive for the duration of the test).
pub fn setup_gd_project(files: &[(&str, &str)]) -> TempDir {
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
