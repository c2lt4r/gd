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

/// Parse autoload entries from a `project.godot` file.
///
/// Returns `(name, res_path)` pairs. The leading `*` (singleton marker) is stripped
/// from the path.  Example: `Game="*res://scripts/global.gd"` → `("Game", "res://scripts/global.gd")`.
pub fn parse_autoloads(project_file: &Path) -> Vec<(String, String)> {
    let Ok(content) = std::fs::read_to_string(project_file) else {
        return Vec::new();
    };

    let mut in_autoload = false;
    let mut result = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_autoload = trimmed == "[autoload]";
            continue;
        }
        if !in_autoload {
            continue;
        }
        // Lines look like: Name="*res://path.gd" or Name="res://path.tscn"
        if let Some((key, val)) = trimmed.split_once('=') {
            let name = key.trim().to_string();
            let mut path = val.trim().trim_matches('"').to_string();
            // Strip leading '*' (singleton marker)
            if let Some(stripped) = path.strip_prefix('*') {
                path = stripped.to_string();
            }
            result.push((name, path));
        }
    }

    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_autoloads_basic() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("project.godot");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(
            f,
            "[application]\nconfig/name=\"Test\"\n\n[autoload]\nGame=\"*res://scripts/global.gd\"\nUI=\"res://ui/hud.tscn\"\n\n[display]\n"
        )
        .unwrap();

        let result = parse_autoloads(&file);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Game");
        assert_eq!(result[0].1, "res://scripts/global.gd");
        assert_eq!(result[1].0, "UI");
        assert_eq!(result[1].1, "res://ui/hud.tscn");
    }

    #[test]
    fn parse_autoloads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("project.godot");
        std::fs::write(&file, "[application]\nconfig/name=\"Test\"\n").unwrap();

        let result = parse_autoloads(&file);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_autoloads_missing_file() {
        let result = parse_autoloads(Path::new("/nonexistent/project.godot"));
        assert!(result.is_empty());
    }
}
