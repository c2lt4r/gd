use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gd_core::config::Config;
use gd_core::fs;
use gd_core::project::GodotProject;
use gd_core::{ceprintln, cprintln};

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
        gd_lsp::daemon_client::query_daemon("cached_godot_path", serde_json::json!({}), None)?;
    let path = result.get("godot_path").and_then(|p| p.as_str())?;
    if path.is_empty() {
        return None;
    }
    if fs::is_wsl() {
        Some(fs::windows_to_wsl_path(path))
    } else {
        Some(path.to_string())
    }
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

/// Generate the GDScript eval server (TCP-based).
/// Listens on a random port, accepts multiple concurrent connections.
/// Wire protocol: 4-byte LE length prefix + payload (same as debug server).
/// Node-based scripts are queued and executed on the next poll tick (one frame delay
/// for `_ready()` to fire). RefCounted scripts execute immediately in the accept loop.
#[allow(clippy::too_many_lines)]
pub fn generate_eval_server(scene_path: &str) -> String {
    format!(
        r#"extends SceneTree

var _root: String
var _tcp: TCPServer
var _queue: Array = []
var _eval_id: int = 0
var _polling: bool = false

func _initialize():
	_root = ProjectSettings.globalize_path("res://")
	# Start TCP server on random port
	_tcp = TCPServer.new()
	_tcp.listen(0, "0.0.0.0")
	var port = _tcp.get_local_port()
	# Write pid:port to ready file (port discovery)
	var f = FileAccess.open(_root.path_join(".godot/gd-eval-ready"), FileAccess.WRITE)
	if f:
		f.store_string("%d:%d" % [OS.get_process_id(), port])
		f.flush()
	change_scene_to_file("{scene_path}")
	var timer = Timer.new()
	timer.name = "GdEvalTimer"
	timer.wait_time = 0.05
	timer.autostart = true
	timer.process_mode = Node.PROCESS_MODE_ALWAYS
	timer.timeout.connect(_poll)
	get_root().call_deferred("add_child", timer)

func _poll():
	if _polling:
		return
	_polling = true
	# 1. Execute all queued runners from previous tick
	while _queue.size() > 0:
		var entry = _queue.pop_front()
		var runner = entry.runner
		var peer = entry.peer
		print("__GD_EVAL_BEGIN__")
		var result = runner.call("run") if is_instance_valid(runner) else null
		if result is Signal:
			result = await result
		print("__GD_EVAL_END__")
		if is_instance_valid(runner):
			runner.queue_free()
		var result_str = str(result) if result != null else ""
		_send_result(peer, result_str, "")

	# 2. Accept and process all available connections
	while _tcp.is_connection_available():
		var peer = _tcp.take_connection()
		if peer != null:
			_accept_eval(peer)
	_polling = false

func _accept_eval(peer: StreamPeerTCP):
	# Read request: [4 bytes LE length][script bytes]
	# StreamPeerTCP.get_data() may return partial data if the full
	# payload hasn't arrived yet (especially on WSL cross-VM TCP).
	# Use _read_exact() to loop until all bytes are received.
	var len_data = _read_exact(peer, 4)
	if len_data == null:
		return
	var script_len = len_data.decode_u32(0)
	var script_data = _read_exact(peer, script_len)
	if script_data == null:
		return
	var source = script_data.get_string_from_utf8()

	# Compile script. If reload() fails and the source has a `return` in
	# run(), retry without it — handles void-returning calls like print(),
	# emit(), etc. that GDScript rejects in `return void_call()`.
	# Note: failed reload() may trigger a debugger SCRIPT ERROR log entry,
	# but it does NOT pause the game (only runtime errors do that).
	var script = GDScript.new()
	script.source_code = source
	var err = script.reload()
	if err != OK:
		var patched = _strip_void_return(source)
		if patched != source:
			script = GDScript.new()
			script.source_code = patched
			err = script.reload()
	if err != OK:
		_send_result(peer, null, "Script compilation failed")
		return

	var obj = script.new()
	if not obj.has_method("run"):
		if obj is Node:
			obj.queue_free()
		_send_result(peer, null, "Script has no run() method")
		return

	if obj is Node:
		_eval_id += 1
		obj.name = "GdEvalRunner_%d" % _eval_id
		obj.process_mode = Node.PROCESS_MODE_ALWAYS
		get_root().add_child(obj)
		_queue.append({{runner = obj, peer = peer}})
	else:
		# RefCounted: execute immediately (no scene tree needed)
		print("__GD_EVAL_BEGIN__")
		var result = obj.call("run")
		if result is Signal:
			result = await result
		print("__GD_EVAL_END__")
		var result_str = str(result) if result != null else ""
		_send_result(peer, result_str, "")

func _strip_void_return(src: String) -> String:
	# If run() body is a single `return <expr>`, strip the return keyword
	# so void-returning calls like emit() or transition_to() compile.
	var lines = src.split("\n")
	for i in lines.size():
		var stripped = lines[i].strip_edges()
		if stripped.begins_with("return ") and i > 0:
			var prev = lines[i - 1].strip_edges()
			if prev == "func run():":
				# Replace first occurrence of "return " preserving indentation
				var idx = lines[i].find("return ")
				lines[i] = lines[i].substr(0, idx) + lines[i].substr(idx + 7)
				return "\n".join(lines)
	return src

func _read_exact(peer: StreamPeerTCP, size: int) -> Variant:
	var buf = PackedByteArray()
	var remaining = size
	var deadline = Time.get_ticks_msec() + 2000
	while remaining > 0:
		if Time.get_ticks_msec() > deadline:
			return null
		peer.poll()
		var chunk = peer.get_partial_data(remaining)
		if chunk[0] != OK:
			return null
		if chunk[1].size() == 0:
			OS.delay_msec(1)
			continue
		buf.append_array(chunk[1])
		remaining -= chunk[1].size()
	return buf

func _send_result(peer: StreamPeerTCP, result, error: String):
	if peer == null:
		return
	var result_str = str(result) if result != null else ""
	var json = JSON.stringify({{"result": result_str, "error": error}})
	var bytes = json.to_utf8_buffer()
	var len_buf = PackedByteArray()
	len_buf.resize(4)
	len_buf.encode_u32(0, bytes.size())
	peer.put_data(len_buf)
	peer.put_data(bytes)

func _finalize():
	if _tcp:
		_tcp.stop()
	DirAccess.remove_absolute(_root.path_join(".godot/gd-eval-ready"))
"#
    )
}

/// Run the Godot project.
#[allow(clippy::too_many_lines, clippy::fn_params_excessive_bools)]
pub fn run_project(
    scene: Option<&str>,
    debug: bool,
    verbose: bool,
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
    let debug_port = match gd_lsp::daemon_client::query_daemon(
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

    cmd.stdin(Stdio::null());

    if eval {
        // Eval mode: redirect output to a file so Godot has valid file handles
        // that survive after the parent process exits. (WSL closes inherited
        // pipe handles when the parent exits, crashing Godot — file handles
        // to a real file avoid this entirely.)
        // Game output is captured via the debug protocol ring buffer (`gd log`).
        let log_path = project.root.join(".godot").join("gd-game.log");
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
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
            let _ = gd_lsp::daemon_client::query_daemon(
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

fn report_game_to_daemon(child: &std::process::Child, eval: bool) {
    let pid = child.id();
    let _ =
        gd_lsp::daemon_client::query_daemon("set_game_pid", serde_json::json!({"pid": pid}), None);
    if eval {
        let _ = gd_lsp::daemon_client::query_daemon(
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
    cprintln!(
        "{} Running {}{debug_info}{eval_info}",
        "▶".green(),
        project_name.bold(),
    );
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
    cprintln!(
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
        ceprintln!("{} Export failed", "✗".red().bold());
        if !stdout.is_empty() {
            ceprintln!("{stdout}");
        }
        if !stderr.is_empty() {
            ceprintln!("{stderr}");
        }
        let code = child_output.status.code().unwrap_or(1);
        std::process::exit(code);
    }

    cprintln!("{} Export complete: {}", "✓".green(), output_file.display());
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
    fn eval_server_contains_tcp_logic() {
        let script = generate_eval_server("res://main.tscn");
        assert!(script.contains("TCPServer"));
        assert!(script.contains("get_local_port"));
        assert!(script.contains("take_connection"));
        assert!(script.contains("gd-eval-ready"));
    }

    #[test]
    fn eval_server_writes_pid_port() {
        let script = generate_eval_server("res://main.tscn");
        assert!(script.contains(r#""%d:%d" % [OS.get_process_id(), port]"#));
    }

    #[test]
    fn eval_server_contains_cleanup() {
        let script = generate_eval_server("res://main.tscn");
        assert!(script.contains("_finalize"));
        assert!(script.contains("_tcp.stop()"));
    }

    #[test]
    fn eval_server_concurrent_queue() {
        let script = generate_eval_server("res://main.tscn");
        // Uses queue instead of single runner slot
        assert!(script.contains("var _queue: Array"));
        assert!(script.contains("_queue.pop_front()"));
        assert!(script.contains("_queue.append("));
        // Accepts all available connections per poll
        assert!(script.contains("while _tcp.is_connection_available()"));
        // Unique runner names
        assert!(script.contains("GdEvalRunner_%d"));
        // Output markers
        assert!(script.contains("__GD_EVAL_BEGIN__"));
        assert!(script.contains("__GD_EVAL_END__"));
    }
}
