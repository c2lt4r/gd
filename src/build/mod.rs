use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::core::config::Config;
use crate::core::project::GodotProject;

/// Binary names to search for on PATH.
const GODOT_BINARY_NAMES: &[&str] = &["godot", "godot4", "godot-4"];

/// Find the Godot binary.
///
/// Search order:
/// 1. `run.godot_path` in gd.toml config
/// 2. `GODOT_PATH` environment variable
/// 3. PATH search for: godot, godot4, godot-4
pub fn find_godot(config: &Config) -> Result<PathBuf> {
    // 1. Check config
    if let Some(ref path) = config.run.godot_path {
        if path.exists() {
            return Ok(path.clone());
        }
        return Err(miette!(
            "Godot binary configured in gd.toml not found: {}",
            path.display()
        ));
    }

    // 2. Check GODOT_PATH env var
    if let Ok(path) = env::var("GODOT_PATH") {
        let path = PathBuf::from(&path);
        if path.exists() {
            return Ok(path);
        }
        return Err(miette!(
            "GODOT_PATH environment variable points to missing file: {}",
            path.display()
        ));
    }

    // 3. Search PATH
    if let Some(path) = search_path() {
        return Ok(path);
    }

    Err(miette!(
        "Could not find Godot binary.\n\
         Searched for {} on PATH.\n\n\
         To fix this, do one of:\n  \
         - Set `run.godot_path` in gd.toml\n  \
         - Set the GODOT_PATH environment variable\n  \
         - Add godot to your PATH",
        GODOT_BINARY_NAMES.join(", ")
    ))
}

/// Search PATH for any known Godot binary name.
fn search_path() -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for name in GODOT_BINARY_NAMES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Run the Godot project.
pub fn run_project(
    scene: Option<&str>,
    debug: bool,
    verbose: bool,
    extra: &[String],
) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let project = GodotProject::discover(&cwd)?;
    let godot = find_godot(&config)?;

    let project_name = project.name().unwrap_or_else(|_| "project".into());
    println!(
        "{} Running {} with {}",
        "▶".green(),
        project_name.bold(),
        godot.display()
    );

    let mut cmd = Command::new(&godot);
    cmd.arg("--path").arg(&project.root);

    if debug {
        cmd.arg("--debug");
    }
    if verbose {
        cmd.arg("--verbose");
    }

    // Extra args from config
    for arg in &config.run.extra_args {
        cmd.arg(arg);
    }

    // Scene path (positional, must come after flags)
    if let Some(scene) = scene {
        cmd.arg(scene);
    }

    // Extra args from CLI (after --)
    for arg in extra {
        cmd.arg(arg);
    }

    cmd.stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit());

    let status = cmd
        .status()
        .map_err(|e| miette!("Failed to start Godot: {e}"))?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}

/// Export/build the Godot project.
pub fn export_project(preset: Option<&str>, output: Option<&str>, release: bool) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let project = GodotProject::discover(&cwd)?;
    let godot = find_godot(&config)?;

    // Determine the export preset
    let available = parse_export_presets(&project.root)?;
    let preset_name = match preset {
        Some(name) => {
            if !available.iter().any(|p| p == name) {
                if available.is_empty() {
                    return Err(miette!(
                        "Export preset '{}' not found.\n\
                         No export presets are configured. \
                         Create them in Godot: Project > Export...",
                        name
                    ));
                }
                return Err(miette!(
                    "Export preset '{}' not found.\nAvailable presets: {}",
                    name,
                    available.join(", ")
                ));
            }
            name.to_string()
        }
        None => {
            if available.is_empty() {
                return Err(miette!(
                    "No export presets configured.\n\
                     Create them in Godot: Project > Export...\n\
                     Then run: gd build --preset <name>"
                ));
            }
            return Err(miette!(
                "No export preset specified.\n\
                 Available presets: {}\n\n\
                 Usage: gd build --preset <name>",
                available.join(", ")
            ));
        }
    };

    // Determine output path
    let output_dir = match output {
        Some(p) => PathBuf::from(p),
        None => project
            .root
            .join(&config.build.output_dir)
            .join(&preset_name),
    };

    // Create output directory
    std::fs::create_dir_all(&output_dir).map_err(|e| {
        miette!(
            "Failed to create output directory {}: {e}",
            output_dir.display()
        )
    })?;

    // Determine output file path (use preset name as default filename)
    let output_file = if output.is_some() {
        // If user gave an explicit path, use it as-is (could be file or dir)
        output_dir.clone()
    } else {
        output_dir.join(&preset_name)
    };

    let export_flag = if release {
        "--export-release"
    } else {
        "--export-debug"
    };

    let mode = if release { "release" } else { "debug" };
    println!(
        "{} Exporting '{}' ({}) to {}",
        "▶".green(),
        preset_name.bold(),
        mode,
        output_file.display()
    );

    // Show a spinner while building
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("invalid spinner template"),
    );
    spinner.set_message("Building...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let mut cmd = Command::new(&godot);
    cmd.arg("--headless")
        .arg("--path")
        .arg(&project.root)
        .arg(export_flag)
        .arg(&preset_name)
        .arg(&output_file);

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child_output = cmd
        .output()
        .map_err(|e| miette!("Failed to start Godot: {e}"))?;

    spinner.finish_and_clear();

    if !child_output.status.success() {
        let stderr = String::from_utf8_lossy(&child_output.stderr);
        let stdout = String::from_utf8_lossy(&child_output.stdout);
        eprintln!("{} Export failed", "✗".red().bold());
        if !stdout.is_empty() {
            eprintln!("{stdout}");
        }
        if !stderr.is_empty() {
            eprintln!("{stderr}");
        }
        let code = child_output.status.code().unwrap_or(1);
        std::process::exit(code);
    }

    println!("{} Export complete: {}", "✓".green(), output_file.display());
    Ok(())
}

/// Parse export_presets.cfg and return the list of preset names.
fn parse_export_presets(project_root: &Path) -> Result<Vec<String>> {
    let presets_file = project_root.join("export_presets.cfg");
    if !presets_file.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&presets_file)
        .map_err(|e| miette!("Failed to read export_presets.cfg: {e}"))?;

    let mut presets = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Look for: name="PresetName"
        if let Some(rest) = trimmed.strip_prefix("name=\"")
            && let Some(name) = rest.strip_suffix('"')
        {
            presets.push(name.to_string());
        }
    }

    Ok(presets)
}
