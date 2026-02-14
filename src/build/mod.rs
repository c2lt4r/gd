use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::core::config::Config;
use crate::core::fs;
use crate::core::project::GodotProject;

/// Binary names to search for on PATH.
const GODOT_BINARY_NAMES: &[&str] = &["godot", "godot4", "godot-4"];

/// Find the Godot binary.
///
/// Search order:
/// 1. `run.godot_path` in gd.toml config
/// 2. `GODOT_PATH` environment variable
/// 3. On WSL: daemon cache (populated from DAP launches)
/// 4. PATH search for: godot, godot4, godot-4
/// 5. On WSL (if no Windows binary found): error with setup instructions
pub fn find_godot(config: &Config) -> Result<PathBuf> {
    // 1. Check config
    if let Some(ref path) = config.run.godot_path {
        let resolved = resolve_configured_path(path);
        if resolved.exists() {
            return Ok(resolved);
        }
        return Err(miette!(
            "Godot binary configured in gd.toml not found: {}",
            path.display()
        ));
    }

    // 2. Check GODOT_PATH env var
    if let Ok(path_str) = env::var("GODOT_PATH") {
        let resolved = resolve_configured_path(&PathBuf::from(&path_str));
        if resolved.exists() {
            return Ok(resolved);
        }
        return Err(miette!(
            "GODOT_PATH environment variable points to missing file: {}",
            resolved.display()
        ));
    }

    // 3. On WSL: check daemon cache (populated from DAP launches)
    if fs::is_wsl()
        && let Some(path) = find_godot_from_daemon()
    {
        return Ok(PathBuf::from(path));
    }

    // 4. Search PATH
    if let Some(path) = search_path() {
        return Ok(path);
    }

    // 5. On WSL: give a WSL-specific error message
    if fs::is_wsl() {
        return Err(miette!(
            "Could not find Windows Godot binary.\n\n\
             Set one of:\n  \
             - `run.godot_path` in gd.toml (e.g. \"C:/path/to/godot.exe\")\n  \
             - GODOT_PATH environment variable\n  \
             - Or run `gd run` once with the Godot editor open to auto-detect"
        ));
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

/// Resolve a configured Godot path. On WSL, converts `C:/...` Windows paths
/// to `/mnt/c/...` so the file existence check works.
fn resolve_configured_path(path: &Path) -> PathBuf {
    if fs::is_wsl() {
        let s = path.to_string_lossy();
        let converted = fs::windows_to_wsl_path(&s);
        if converted != s.as_ref() {
            return PathBuf::from(converted);
        }
    }
    path.to_path_buf()
}

/// Query the daemon for a cached Godot binary path (from previous DAP launches).
/// Returns the WSL-converted path if found.
fn find_godot_from_daemon() -> Option<String> {
    let result =
        crate::lsp::daemon_client::query_daemon("dap_godot_path", serde_json::json!({}), None)?;
    let path = result.get("godot_path").and_then(|p| p.as_str())?;
    if path.is_empty() {
        return None;
    }
    Some(fs::windows_to_wsl_path(path))
}

/// Get the `--path` argument for Godot, translating WSL paths for Windows binaries.
fn project_path_for_godot(godot: &Path, project_root: &Path) -> String {
    if fs::is_windows_binary(godot) {
        let s = project_root.to_string_lossy();
        fs::wsl_to_windows_path(&s).unwrap_or_else(|| s.to_string())
    } else {
        project_root.to_string_lossy().to_string()
    }
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

    let mut cmd = Command::new(&godot);
    cmd.arg("--path")
        .arg(project_path_for_godot(&godot, &project.root));

    // Always wire up remote debug via daemon (silent — no user-facing port args)
    let debug_port = match crate::lsp::daemon_client::query_daemon(
        "debug_start_server",
        serde_json::json!({}),
        None,
    ) {
        Some(result) => {
            let port = result.get("port").and_then(|p| p.as_u64()).unwrap_or(0);
            if port > 0 {
                cmd.arg("--remote-debug")
                    .arg(format!("tcp://127.0.0.1:{port}"));
                Some(port)
            } else {
                None
            }
        }
        None => None,
    };

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

    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| miette!("Failed to start Godot: {e}"))?;

    let pid = child.id();

    // Report PID to daemon so `gd debug stop` can kill the game
    let _ = crate::lsp::daemon_client::query_daemon(
        "set_game_pid",
        serde_json::json!({"pid": pid}),
        None,
    );

    // Reap the child in a background thread to avoid zombies
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    // Tell the daemon to accept the debug connection (fire-and-forget)
    if debug_port.is_some() {
        std::thread::spawn(|| {
            let _ = crate::lsp::daemon_client::query_daemon(
                "debug_accept",
                serde_json::json!({"timeout": 30}),
                Some(std::time::Duration::from_secs(35)),
            );
        });
        // Give the daemon query time to send before process exit
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Print clean status line
    let debug_info = if let Some(port) = debug_port {
        format!(" (debug on port {port})")
    } else {
        String::new()
    };
    println!(
        "{} Running {}{debug_info}",
        "▶".green(),
        project_name.bold(),
    );

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
        output_dir
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
        .arg(project_path_for_godot(&godot, &project.root))
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
