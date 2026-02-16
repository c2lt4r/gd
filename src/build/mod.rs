use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

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
/// 3. On WSL: daemon cache (populated from debug launches)
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

    // 3. On WSL: check daemon cache (populated from debug launches)
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

/// Query the daemon for a cached Godot binary path.
/// Returns the WSL-converted path if found.
fn find_godot_from_daemon() -> Option<String> {
    let result =
        crate::lsp::daemon_client::query_daemon("cached_godot_path", serde_json::json!({}), None)?;
    let path = result.get("godot_path").and_then(|p| p.as_str())?;
    if path.is_empty() {
        return None;
    }
    Some(fs::windows_to_wsl_path(path))
}

/// Get a path argument for Godot, translating WSL paths for Windows binaries.
pub fn path_for_godot(godot: &Path, path: &Path) -> String {
    if fs::is_windows_binary(godot) {
        let s = path.to_string_lossy();
        fs::wsl_to_windows_path(&s).unwrap_or_else(|| s.to_string())
    } else {
        path.to_string_lossy().to_string()
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

/// Generate the GDScript eval server that polls for eval requests.
/// `scene_path` is the `res://...` path to the main scene to load.
///
/// Uses `_initialize()` + `process_frame` signal instead of overriding `_process()`
/// to avoid breaking SceneTree's built-in frame processing.
pub fn generate_eval_server(scene_path: &str) -> String {
    format!(
        r##"extends SceneTree

var _root: String
var _runner: Node = null
var _eval_id: String = ""

func _initialize():
	_root = ProjectSettings.globalize_path("res://")
	var f = FileAccess.open(_root.path_join(".godot/gd-eval-ready"), FileAccess.WRITE)
	if f:
		f.store_string(str(OS.get_process_id()))
		f.flush()
	change_scene_to_file("{scene_path}")
	# Use a Timer for polling — its callbacks fire during idle phase,
	# which is safe for tree modification (unlike process_frame).
	var timer = Timer.new()
	timer.name = "GdEvalTimer"
	timer.wait_time = 0.05
	timer.autostart = true
	timer.process_mode = Node.PROCESS_MODE_ALWAYS
	timer.timeout.connect(_poll)
	get_root().call_deferred("add_child", timer)

func _poll():
	# State machine: if a runner is pending, execute it (it's been in tree 1+ frames)
	if _runner != null:
		var result = _runner.call("run")
		if is_instance_valid(_runner):
			_runner.queue_free()
		_runner = null
		var result_str = str(result) if result != null else ""
		_write_result(JSON.stringify({{"result": result_str, "error": ""}}))
		return

	# Scan for per-ID request files (gd-eval-request-*.gd)
	var godot_dir = _root.path_join(".godot")
	var dir = DirAccess.open(godot_dir)
	if dir == null:
		return
	var req_file := ""
	dir.list_dir_begin()
	var fname = dir.get_next()
	while fname != "":
		if fname.begins_with("gd-eval-request-") and fname.ends_with(".gd"):
			req_file = fname
			break
		fname = dir.get_next()
	dir.list_dir_end()
	if req_file.is_empty():
		return

	var req = godot_dir.path_join(req_file)

	# Read the request file
	var file = FileAccess.open(req, FileAccess.READ)
	if file == null:
		# File may be locked by the writer — skip and retry next cycle
		return
	var source = file.get_as_text()
	file = null

	# Delete the request file. If this fails (cross-filesystem race), skip this
	# request — the Rust side will clean up stale files before its next eval call.
	var err_del = DirAccess.remove_absolute(req)
	if err_del != OK:
		return

	# Extract request ID from first line: # eval-id: <id>
	_eval_id = ""
	var first_line = source.get_slice("\n", 0)
	if first_line.begins_with("# eval-id: "):
		_eval_id = first_line.substr(11).strip_edges()

	var script = GDScript.new()
	script.source_code = source
	var err = script.reload()
	if err != OK:
		_write_result('{{"result":null,"error":"Script compilation failed"}}')
		return

	var runner = Node.new()
	runner.name = "GdEvalRunner"
	runner.process_mode = Node.PROCESS_MODE_ALWAYS
	runner.set_script(script)
	if not runner.has_method("run"):
		runner.queue_free()
		_write_result('{{"result":null,"error":"Script has no run() method"}}')
		return

	# Add to tree — run() will be called next poll cycle via state machine
	get_root().add_child(runner)
	_runner = runner

func _write_result(json_str: String):
	var name = "gd-eval-result.json"
	if not _eval_id.is_empty():
		name = "gd-eval-result-" + _eval_id + ".json"
	var f = FileAccess.open(_root.path_join(".godot/" + name), FileAccess.WRITE)
	if f:
		f.store_string(json_str)
		f.flush()

func _finalize():
	DirAccess.remove_absolute(_root.path_join(".godot/gd-eval-ready"))
"##
    )
}

/// Run the Godot project.
#[allow(clippy::too_many_lines, clippy::fn_params_excessive_bools)]
pub fn run_project(
    scene: Option<&str>,
    debug: bool,
    verbose: bool,
    log: bool,
    eval: bool,
    extra: &[String],
) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let project = GodotProject::discover(&cwd)?;
    let godot = find_godot(&config)?;

    let project_name = project.name().unwrap_or_else(|_| "project".into());

    let mut cmd = Command::new(&godot);
    cmd.arg("--path").arg(path_for_godot(&godot, &project.root));

    // Always wire up remote debug via daemon (silent — no user-facing port args)
    let debug_port = match crate::lsp::daemon_client::query_daemon(
        "debug_start_server",
        serde_json::json!({}),
        None,
    ) {
        Some(result) => {
            let port = result
                .get("port")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
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

    // Eval server mode: inject --script with eval server, bake in the scene
    if eval {
        let scene_path = if let Some(s) = scene {
            // User passed an explicit scene — use it as-is (res:// path or relative)
            if s.starts_with("res://") {
                s.to_string()
            } else {
                format!("res://{s}")
            }
        } else {
            project.main_scene()?.ok_or_else(|| {
                miette!(
                    "No main scene configured and no scene argument provided.\n\
                     Set run/main_scene in project.godot or pass a scene: gd run <scene>"
                )
            })?
        };

        let server_script = generate_eval_server(&scene_path);
        let server_path = project.root.join(".godot").join("gd-eval-server.gd");
        std::fs::create_dir_all(project.root.join(".godot"))
            .map_err(|e| miette!("Failed to create .godot directory: {e}"))?;
        std::fs::write(&server_path, &server_script)
            .map_err(|e| miette!("Failed to write eval server script: {e}"))?;

        cmd.arg("--script")
            .arg(path_for_godot(&godot, &server_path));
    } else {
        // Scene path (positional, must come after flags)
        if let Some(scene) = scene {
            cmd.arg(scene);
        }
    }

    // Extra args from CLI (after --)
    for arg in extra {
        cmd.arg(arg);
    }

    // Always capture output to log file
    let log_path = log_file_path(&project.root);
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    cmd.stdin(Stdio::null());

    if log {
        // Pipe stdout/stderr so we can write to log file AND print to terminal.
        // The tail loop keeps the main process alive, so pipes stay open.
        let log_file = std::fs::File::create(&log_path)
            .map_err(|e| miette!("Failed to create log file: {e}"))?;
        let log_file = Arc::new(std::sync::Mutex::new(std::io::LineWriter::new(log_file)));

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| miette!("Failed to start Godot: {e}"))?;

        report_game_to_daemon(&child, eval);

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        // Pump stdout/stderr to log file in background threads
        let log1 = Arc::clone(&log_file);
        std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut f) = log1.lock() {
                    let _ = writeln!(f, "{line}");
                }
            }
        });

        let log2 = Arc::clone(&log_file);
        std::thread::spawn(move || {
            use std::io::{BufRead, Write};
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut f) = log2.lock() {
                    let _ = writeln!(f, "{line}");
                }
            }
        });

        // Reap the child in the background
        std::thread::spawn(move || {
            let _ = child.wait();
        });

        print_status_line(&project_name, debug_port, eval);

        // Tail the log file (blocks until game exits or Ctrl+C)
        tail_log_file(&log_path);
    } else if eval {
        // Eval mode without --log: redirect output to the log file so Godot
        // has valid file handles that survive after the parent process exits.
        // (WSL closes inherited pipe handles when the parent exits, crashing
        // Godot — file handles to a real file avoid this entirely.)
        let out_file = std::fs::File::create(&log_path)
            .map_err(|e| miette!("Failed to create log file: {e}"))?;
        let err_file = out_file
            .try_clone()
            .map_err(|e| miette!("Failed to clone log handle: {e}"))?;
        cmd.stdout(out_file).stderr(err_file);

        let child = cmd
            .spawn()
            .map_err(|e| miette!("Failed to start Godot: {e}"))?;

        report_game_to_daemon(&child, eval);

        // Reap the child in the background to avoid zombies
        std::thread::spawn(move || {
            let _ = child.wait_with_output();
        });

        print_status_line(&project_name, debug_port, eval);
    } else {
        // Normal run: inherit terminal handles and exit immediately
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

        let child = cmd
            .spawn()
            .map_err(|e| miette!("Failed to start Godot: {e}"))?;

        report_game_to_daemon(&child, eval);

        // Reap the child in the background to avoid zombies
        std::thread::spawn(move || {
            let _ = child.wait_with_output();
        });

        print_status_line(&project_name, debug_port, eval);
    }

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

    Ok(())
}

/// Tail a log file to stdout, following new content as it's written.
/// Returns when the game process exits (detected via daemon) or on Ctrl+C.
fn tail_log_file(path: &Path) {
    use std::io::{BufRead as _, Seek, SeekFrom};

    // Set up Ctrl+C handler to exit cleanly (game keeps running)
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = Arc::clone(&running);
    let _ = ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::Release);
    });

    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let mut reader = BufReader::new(file);
    // Start from the beginning to show existing output
    let _ = reader.seek(SeekFrom::Start(0));

    let mut line = String::new();
    while running.load(std::sync::atomic::Ordering::Acquire) {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // No new data — check if game is still running
                let game_alive =
                    crate::lsp::daemon_client::query_daemon("status", serde_json::json!({}), None)
                        .and_then(|r| r.get("game_running").and_then(serde_json::Value::as_bool))
                        .unwrap_or(false);
                if !game_alive {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Ok(_) => {
                // Print without extra newline (read_line includes \n)
                print!("{line}");
            }
            Err(_) => break,
        }
    }
}

fn report_game_to_daemon(child: &std::process::Child, eval: bool) {
    let pid = child.id();
    let _ = crate::lsp::daemon_client::query_daemon(
        "set_game_pid",
        serde_json::json!({"pid": pid}),
        None,
    );
    if eval {
        let _ = crate::lsp::daemon_client::query_daemon(
            "set_eval_mode",
            serde_json::json!({"enabled": true}),
            None,
        );
    }
}

fn print_status_line(project_name: &str, debug_port: Option<u64>, eval: bool) {
    let debug_info = if let Some(port) = debug_port {
        format!(" (debug on port {port})")
    } else {
        String::new()
    };
    let eval_info = if eval {
        ""
    } else {
        " (bare — no eval server)"
    };
    println!(
        "{} Running {}{debug_info}{eval_info}",
        "▶".green(),
        project_name.bold(),
    );
}

/// Path to the game log file within a project.
pub fn log_file_path(project_root: &Path) -> PathBuf {
    project_root.join(".godot").join("gd-game.log")
}

/// Export/build the Godot project.
#[allow(clippy::too_many_lines)]
pub fn export_project(preset: Option<&str>, output: Option<&str>, release: bool) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let project = GodotProject::discover(&cwd)?;
    let godot = find_godot(&config)?;

    // Determine the export preset
    let available = parse_export_presets(&project.root)?;
    let preset_name = if let Some(name) = preset {
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
    } else {
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
        .arg(path_for_godot(&godot, &project.root))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_server_contains_scene_load() {
        let script = generate_eval_server("res://main.tscn");
        assert!(script.contains(r#"change_scene_to_file("res://main.tscn")"#));
        assert!(script.contains("extends SceneTree"));
    }

    #[test]
    fn eval_server_contains_poll_logic() {
        let script = generate_eval_server("res://main.tscn");
        assert!(script.contains("gd-eval-request-"));
        assert!(script.contains("gd-eval-result.json"));
        assert!(script.contains("gd-eval-ready"));
    }

    #[test]
    fn eval_server_contains_cleanup() {
        let script = generate_eval_server("res://main.tscn");
        assert!(script.contains("_finalize"));
        assert!(script.contains("gd-eval-ready"));
    }
}
