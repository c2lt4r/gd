use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::fs;
use std::path::Path;

use crate::core::project::GodotProject;
use crate::scaffold::templates::GD_TOML_TEMPLATE;

#[derive(Args)]
pub struct InitArgs {
    /// Overwrite existing gd.toml
    #[arg(long)]
    pub force: bool,
}

pub fn exec(args: InitArgs) -> Result<()> {
    let cwd = env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;

    let project = GodotProject::discover(&cwd)?;
    let config_path = project.root.join("gd.toml");

    if config_path.exists() && !args.force {
        return Err(miette!(
            "gd.toml already exists at {}\nUse {} to overwrite",
            config_path.display(),
            "--force".bold()
        ));
    }

    let mut template = GD_TOML_TEMPLATE.to_owned();

    // Detect output directory from export_presets.cfg if it exists
    if let Some(output_dir) = detect_output_dir(&project.root) {
        template = template.replace(
            "output_dir = \"build\"",
            &format!("output_dir = \"{}\"", output_dir),
        );
    }

    fs::write(&config_path, template).map_err(|e| miette!("Failed to write gd.toml: {e}"))?;

    let name = project.name()?;

    println!(
        "{} gd toolchain in project {}",
        "Initialized".green().bold(),
        name.bold()
    );
    println!("  Project root: {}", project.root.display());
    println!("  Config: {}", config_path.display());

    Ok(())
}

/// Detect the build output directory from Godot's export_presets.cfg.
///
/// Parses all `export_path` entries and returns the common top-level
/// directory if all presets share one (e.g. "bin" if all paths start
/// with "bin/..."). Returns None if no presets exist or paths disagree.
fn detect_output_dir(project_root: &Path) -> Option<String> {
    let presets_file = project_root.join("export_presets.cfg");
    let content = fs::read_to_string(presets_file).ok()?;

    let dirs: Vec<&str> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("export_path=\"")?;
            let path = rest.strip_suffix('"')?;
            if path.is_empty() {
                return None;
            }
            // Extract first path component (e.g. "bin" from "bin/client/game.exe")
            // Handle both forward and back slashes (Windows)
            path.split(['/', '\\']).next()
        })
        .collect();

    if dirs.is_empty() {
        return None;
    }

    let first = dirs[0];
    // All export paths must share the same top-level directory
    if first != "build" && dirs.iter().all(|d| *d == first) {
        Some(first.to_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detect_output_dir_from_export_presets() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("export_presets.cfg"),
            "[preset.0]\nexport_path=\"bin/client/game.exe\"\n\n[preset.1]\nexport_path=\"bin/server/game.x86_64\"\n",
        ).unwrap();
        assert_eq!(detect_output_dir(dir.path()), Some("bin".into()));
    }

    #[test]
    fn detect_output_dir_disagreeing_paths() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("export_presets.cfg"),
            "[preset.0]\nexport_path=\"bin/game.exe\"\n\n[preset.1]\nexport_path=\"out/game.x86_64\"\n",
        ).unwrap();
        assert_eq!(detect_output_dir(dir.path()), None);
    }

    #[test]
    fn detect_output_dir_no_presets_file() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_output_dir(dir.path()), None);
    }

    #[test]
    fn detect_output_dir_empty_export_paths() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("export_presets.cfg"),
            "[preset.0]\nexport_path=\"\"\n",
        )
        .unwrap();
        assert_eq!(detect_output_dir(dir.path()), None);
    }

    #[test]
    fn detect_output_dir_backslash_paths() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("export_presets.cfg"),
            "[preset.0]\nexport_path=\"bin\\client\\game.exe\"\n\n[preset.1]\nexport_path=\"bin\\server\\game.x86_64\"\n",
        ).unwrap();
        assert_eq!(detect_output_dir(dir.path()), Some("bin".into()));
    }

    #[test]
    fn detect_output_dir_skips_build_default() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("export_presets.cfg"),
            "[preset.0]\nexport_path=\"build/game.exe\"\n",
        )
        .unwrap();
        // "build" is already the default, no need to override
        assert_eq!(detect_output_dir(dir.path()), None);
    }
}
