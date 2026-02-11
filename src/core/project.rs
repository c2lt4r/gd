use miette::{Result, miette};
use std::path::{Path, PathBuf};
use std::process::Command;

const PROJECT_FILE: &str = "project.godot";
const DEFAULT_GODOT_VERSION: &str = "4.0";

/// Represents a discovered Godot project.
#[derive(Debug)]
pub struct GodotProject {
    /// Path to the project.godot file.
    pub project_file: PathBuf,
    /// Root directory of the project.
    pub root: PathBuf,
}

impl GodotProject {
    /// Find the Godot project by searching upward from `start`.
    pub fn discover(start: &Path) -> Result<Self> {
        find_project(start).ok_or_else(|| {
            miette!("No Godot project found (no project.godot in any parent directory)")
        })
    }

    /// Get the project name from project.godot.
    pub fn name(&self) -> Result<String> {
        let content = std::fs::read_to_string(&self.project_file)
            .map_err(|e| miette!("Failed to read project.godot: {e}"))?;
        for line in content.lines() {
            if let Some(name) = line.strip_prefix("config/name=\"") {
                return Ok(name.trim_end_matches('"').to_string());
            }
        }
        Ok("Untitled".to_string())
    }
}

/// Detect the installed Godot major.minor version (e.g. "4.6").
/// Tries GODOT_PATH env, then PATH, falls back to "4.0".
pub fn detect_godot_version() -> String {
    let binary = std::env::var("GODOT_PATH")
        .ok()
        .unwrap_or_else(|| "godot".to_string());

    Command::new(&binary)
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            let version_str = String::from_utf8_lossy(&out.stdout);
            // Output format: "4.6.stable.official.89cea1439"
            let parts: Vec<&str> = version_str.trim().splitn(3, '.').collect();
            if parts.len() >= 2 {
                Some(format!("{}.{}", parts[0], parts[1]))
            } else {
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_GODOT_VERSION.to_string())
}

/// Walk upward from `start` looking for project.godot.
fn find_project(start: &Path) -> Option<GodotProject> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidate = dir.join(PROJECT_FILE);
        if candidate.is_file() {
            return Some(GodotProject {
                project_file: candidate,
                root: dir,
            });
        }
        if !dir.pop() {
            return None;
        }
    }
}
