mod dispatch_debug;
mod dispatch_env;
mod dispatch_live;
mod dispatch_lsp;
mod helpers;

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use super::godot_client::GodotClient;
use super::workspace::WorkspaceIndex;

/// State file written to `.godot/gd-daemon.json` so CLI clients can find us.
#[derive(Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub port: u16,
    /// PID of the game process launched by `gd run` (persisted for `gd stop`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_pid: Option<u32>,
    /// Build fingerprint so clients can detect stale daemons after recompilation.
    #[serde(default)]
    pub build_id: String,
}

/// Fingerprint of the current binary (version + mtime).
/// Changes whenever the binary is recompiled.
pub fn current_build_id() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mtime = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    format!("{version}-{mtime}")
}

/// A daemon request sent over TCP.
#[derive(Deserialize)]
struct DaemonRequest {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// A daemon response sent over TCP.
#[derive(Serialize)]
struct DaemonResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[allow(dead_code)] // workspace/project_root used by dispatch_* through query::*
struct DaemonServer {
    godot: Mutex<Option<GodotClient>>,
    godot_ready: std::sync::atomic::AtomicBool,
    /// True when a game is running (for idle timeout).
    game_running: Arc<std::sync::atomic::AtomicBool>,
    debug_server: Mutex<Option<Arc<crate::debug::godot_debug_server::GodotDebugServer>>>,
    /// PID of the game process launched by `gd run` (for `gd debug stop`).
    game_pid: Mutex<Option<u32>>,
    /// Cached Godot binary path (Windows path in WSL).
    cached_godot_path: Mutex<Option<String>>,
    workspace: WorkspaceIndex,
    project_root: PathBuf,
    godot_port: u16,
    last_activity: Mutex<Instant>,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Entry point for `gd lsp daemon`. Runs a persistent background server.
pub fn run(project_root: &Path, godot_port: u16) -> miette::Result<()> {
    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| miette::miette!("cannot bind TCP listener: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| miette::miette!("cannot get local address: {e}"))?
        .port();

    // Build workspace index for cross-file resolution
    let workspace = WorkspaceIndex::new(project_root.to_path_buf());

    let server = std::sync::Arc::new(DaemonServer {
        godot: Mutex::new(None),
        godot_ready: std::sync::atomic::AtomicBool::new(false),
        game_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        debug_server: Mutex::new(None),
        game_pid: Mutex::new(None),
        cached_godot_path: Mutex::new(None),
        workspace,
        project_root: project_root.to_path_buf(),
        godot_port,
        last_activity: Mutex::new(Instant::now()),
    });

    // Write state file immediately so clients can connect
    let state_path = project_root.join(".godot").join("gd-daemon.json");
    write_state_file(&state_path, port, None)?;

    // Connect to Godot LSP in a background thread (handshake can be slow)
    {
        let server_init = std::sync::Arc::clone(&server);
        let root = project_root.to_path_buf();
        std::thread::spawn(move || {
            if godot_port > 0
                && let Some(client) = GodotClient::connect("127.0.0.1", godot_port)
            {
                client.initialize(&root);
                *server_init.godot.lock().unwrap() = Some(client);
            }
            server_init
                .godot_ready
                .store(true, std::sync::atomic::Ordering::Release);
        });
    }

    // Spawn idle monitor thread
    let state_path_clone = state_path.clone();
    let server_idle = std::sync::Arc::clone(&server);
    std::thread::spawn(move || idle_monitor(&server_idle, &state_path_clone));

    // Accept connections — spawn threads for blocking requests
    for conn in listener.incoming().flatten() {
        *server.last_activity.lock().unwrap() = Instant::now();
        let srv = std::sync::Arc::clone(&server);
        let sp = state_path.clone();
        std::thread::spawn(move || {
            if let Some(should_exit) = handle_connection(&srv, conn)
                && should_exit
            {
                let _ = std::fs::remove_file(&sp);
                std::process::exit(0);
            }
        });
    }

    Ok(())
}

fn write_state_file(path: &Path, port: u16, game_pid: Option<u32>) -> miette::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| miette::miette!("cannot create state directory: {e}"))?;
    }
    let state = DaemonState {
        pid: std::process::id(),
        port,
        game_pid,
        build_id: current_build_id(),
    };
    let json =
        serde_json::to_string_pretty(&state).map_err(|e| miette::miette!("serialize: {e}"))?;
    std::fs::write(path, json).map_err(|e| miette::miette!("cannot write state file: {e}"))?;
    Ok(())
}

/// Clear the game_pid field in the state file (public for `gd stop` fallback).
pub fn clear_game_pid_in_state(project_root: &Path) {
    update_game_pid_in_state(project_root, None);
}

/// Update just the game_pid field in the state file without touching other fields.
fn update_game_pid_in_state(project_root: &Path, game_pid: Option<u32>) {
    let path = project_root.join(".godot").join("gd-daemon.json");
    if let Ok(data) = std::fs::read_to_string(&path)
        && let Ok(mut state) = serde_json::from_str::<DaemonState>(&data)
    {
        state.game_pid = game_pid;
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(&path, json);
        }
    }
}

fn idle_monitor(server: &DaemonServer, state_path: &Path) {
    loop {
        std::thread::sleep(IDLE_CHECK_INTERVAL);
        // Never exit while a game is running
        if server
            .game_running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            continue;
        }
        let elapsed = server.last_activity.lock().unwrap().elapsed();
        if elapsed >= IDLE_TIMEOUT {
            let _ = std::fs::remove_file(state_path);
            std::process::exit(0);
        }
    }
}

/// Handle a single TCP connection. Returns `Some(true)` for shutdown.
fn handle_connection(server: &DaemonServer, mut stream: TcpStream) -> Option<bool> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok()?;

    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let request = read_request(&mut reader)?;

    // debug_accept can block — extend timeout
    if request.method == "debug_accept" {
        let timeout = request
            .params
            .get("timeout")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(30);
        stream
            .set_read_timeout(Some(Duration::from_secs(timeout + 5)))
            .ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(timeout + 5)))
            .ok()?;
    }

    let response = dispatch(server, &request);
    let should_exit = request.method == "shutdown";
    write_response(&mut stream, &response).ok()?;
    Some(should_exit)
}

#[allow(clippy::too_many_lines)]
fn dispatch(server: &DaemonServer, request: &DaemonRequest) -> DaemonResponse {
    match request.method.as_str() {
        // LSP queries
        "hover" => dispatch_lsp::dispatch_hover(server, &request.params),
        "completion" => dispatch_lsp::dispatch_completion(server, &request.params),
        "definition" => dispatch_lsp::dispatch_definition(server, &request.params),
        // Daemon status
        "status" => dispatch_status(server),
        // Project path (from Godot LSP URI mapping)
        "godot_project_path" => dispatch_godot_project_path(server),
        // Godot binary path cache (WSL probe)
        "cached_godot_path" => dispatch_cached_godot_path(server),
        "cache_godot_path" => dispatch_cache_godot_path(server, &request.params),
        // Binary debug protocol
        "set_game_pid" => dispatch_debug::dispatch_set_game_pid(server, &request.params),
        "debug_stop_game" => dispatch_debug::dispatch_debug_stop_game(server),
        "debug_start_server" => {
            dispatch_debug::dispatch_debug_start_server(server, &request.params)
        }
        "debug_accept" => dispatch_debug::dispatch_debug_accept(server, &request.params),
        "debug_scene_tree" => dispatch_debug::dispatch_debug_scene_tree(server),
        "debug_inspect" => dispatch_debug::dispatch_debug_inspect(server, &request.params),
        "debug_set_property" => {
            dispatch_debug::dispatch_debug_set_property(server, &request.params)
        }
        "debug_suspend" => dispatch_debug::dispatch_debug_suspend(server, &request.params),
        "debug_next_frame" => dispatch_debug::dispatch_debug_next_frame(server),
        "debug_time_scale" => dispatch_debug::dispatch_debug_time_scale(server, &request.params),
        "debug_reload_scripts" => {
            dispatch_debug::dispatch_debug_reload_scripts(server, &request.params)
        }
        "debug_server_status" => dispatch_debug::dispatch_debug_server_status(server),
        "debug_is_at_breakpoint" => dispatch_debug::dispatch_debug_is_at_breakpoint(server),
        // Core debugger (binary protocol)
        "debug_continue" => dispatch_debug::dispatch_debug_cmd_simple(server, "continue"),
        "debug_break_exec" => dispatch_debug::dispatch_debug_cmd_simple(server, "break"),
        "debug_next_step" => dispatch_debug::dispatch_debug_cmd_simple(server, "next"),
        "debug_step_in" => dispatch_debug::dispatch_debug_cmd_simple(server, "step"),
        "debug_step_out" => dispatch_debug::dispatch_debug_cmd_simple(server, "out"),
        "debug_breakpoint" => dispatch_debug::dispatch_debug_breakpoint(server, &request.params),
        "debug_set_skip_breakpoints" => {
            dispatch_debug::dispatch_debug_bool_cmd(server, "skip_breakpoints", &request.params)
        }
        "debug_set_ignore_error_breaks" => {
            dispatch_debug::dispatch_debug_bool_cmd(server, "ignore_error_breaks", &request.params)
        }
        "debug_get_stack_dump" => dispatch_debug::dispatch_debug_get_stack_dump(server),
        "debug_get_stack_frame_vars" => {
            dispatch_debug::dispatch_debug_get_stack_frame_vars(server, &request.params)
        }
        "debug_evaluate" => dispatch_debug::dispatch_debug_evaluate(server, &request.params),
        "debug_reload_all_scripts" => {
            dispatch_debug::dispatch_debug_simple(server, "reload_all_scripts")
        }
        // Scene inspection
        "debug_inspect_objects" => {
            dispatch_debug::dispatch_debug_inspect_objects(server, &request.params)
        }
        "debug_clear_selection" => dispatch_debug::dispatch_debug_simple(server, "clear_selection"),
        "debug_save_node" => dispatch_debug::dispatch_debug_save_node(server, &request.params),
        "debug_set_property_field" => {
            dispatch_debug::dispatch_debug_set_property_field(server, &request.params)
        }
        // Audio
        "debug_mute_audio" => dispatch_env::dispatch_debug_mute_audio(server, &request.params),
        // File reload
        "debug_reload_cached_files" => {
            dispatch_env::dispatch_debug_reload_cached_files(server, &request.params)
        }
        // Camera override
        "debug_override_cameras" => {
            dispatch_env::dispatch_debug_override_cameras(server, &request.params)
        }
        "debug_transform_camera_2d" => {
            dispatch_env::dispatch_debug_transform_camera_2d(server, &request.params)
        }
        "debug_transform_camera_3d" => {
            dispatch_env::dispatch_debug_transform_camera_3d(server, &request.params)
        }
        // Screenshots
        "debug_request_screenshot" => {
            dispatch_env::dispatch_debug_request_screenshot(server, &request.params)
        }
        // Runtime node selection
        "debug_node_select_set_type" => dispatch_debug::dispatch_debug_int_cmd(
            server,
            "node_select_type",
            &request.params,
            "type",
        ),
        "debug_node_select_set_mode" => dispatch_debug::dispatch_debug_int_cmd(
            server,
            "node_select_mode",
            &request.params,
            "mode",
        ),
        "debug_node_select_set_visible" => dispatch_debug::dispatch_debug_bool_param(
            server,
            "node_select_visible",
            &request.params,
            "visible",
        ),
        "debug_node_select_set_avoid_locked" => dispatch_debug::dispatch_debug_bool_param(
            server,
            "node_select_avoid_locked",
            &request.params,
            "avoid",
        ),
        "debug_node_select_set_prefer_group" => dispatch_debug::dispatch_debug_bool_param(
            server,
            "node_select_prefer_group",
            &request.params,
            "prefer",
        ),
        "debug_node_select_reset_camera_2d" => {
            dispatch_debug::dispatch_debug_simple(server, "node_select_reset_2d")
        }
        "debug_node_select_reset_camera_3d" => {
            dispatch_debug::dispatch_debug_simple(server, "node_select_reset_3d")
        }
        // Live editing
        "debug_live_set_root" => {
            dispatch_live::dispatch_debug_live_set_root(server, &request.params)
        }
        "debug_live_node_path" => {
            dispatch_live::dispatch_debug_live_path(server, "node_path", &request.params)
        }
        "debug_live_res_path" => {
            dispatch_live::dispatch_debug_live_path(server, "res_path", &request.params)
        }
        "debug_live_node_prop" => {
            dispatch_live::dispatch_debug_live_prop(server, "node_prop", &request.params)
        }
        "debug_live_node_prop_res" => {
            dispatch_live::dispatch_debug_live_prop_res(server, "node_prop_res", &request.params)
        }
        "debug_live_res_prop" => {
            dispatch_live::dispatch_debug_live_prop(server, "res_prop", &request.params)
        }
        "debug_live_res_prop_res" => {
            dispatch_live::dispatch_debug_live_prop_res(server, "res_prop_res", &request.params)
        }
        "debug_live_node_call" => {
            dispatch_live::dispatch_debug_live_call(server, "node_call", &request.params)
        }
        "debug_live_res_call" => {
            dispatch_live::dispatch_debug_live_call(server, "res_call", &request.params)
        }
        "debug_live_create_node" => {
            dispatch_live::dispatch_debug_live_create_node(server, &request.params)
        }
        "debug_live_instantiate_node" => {
            dispatch_live::dispatch_debug_live_instantiate_node(server, &request.params)
        }
        "debug_live_remove_node" => {
            dispatch_live::dispatch_debug_live_single_path(server, "remove_node", &request.params)
        }
        "debug_live_remove_and_keep_node" => {
            dispatch_live::dispatch_debug_live_remove_and_keep(server, &request.params)
        }
        "debug_live_restore_node" => {
            dispatch_live::dispatch_debug_live_restore_node(server, &request.params)
        }
        "debug_live_duplicate_node" => {
            dispatch_live::dispatch_debug_live_duplicate_node(server, &request.params)
        }
        "debug_live_reparent_node" => {
            dispatch_live::dispatch_debug_live_reparent_node(server, &request.params)
        }
        // Profiler
        "debug_toggle_profiler" => {
            dispatch_env::dispatch_debug_toggle_profiler(server, &request.params)
        }
        // Control
        "shutdown" => DaemonResponse {
            result: Some(serde_json::json!({"status": "shutdown"})),
            error: None,
        },
        other => DaemonResponse {
            result: None,
            error: Some(format!("unknown method: {other}")),
        },
    }
}

// ── Status & path dispatch (kept in mod.rs — small, access DaemonServer fields directly) ─

fn dispatch_status(server: &DaemonServer) -> DaemonResponse {
    let godot_connected = server.godot.lock().unwrap().is_some();
    let godot_ready = server
        .godot_ready
        .load(std::sync::atomic::Ordering::Acquire);
    let godot_path = server
        .godot
        .lock()
        .unwrap()
        .as_ref()
        .and_then(super::godot_client::GodotClient::godot_project_path);
    let game_running = server
        .game_running
        .load(std::sync::atomic::Ordering::Acquire);
    ok_response(serde_json::json!({
        "godot_connected": godot_connected,
        "godot_ready": godot_ready,
        "godot_project_path": godot_path,
        "game_running": game_running,
        "project_root": server.project_root.to_string_lossy(),
        "godot_port": server.godot_port,
    }))
}

fn dispatch_godot_project_path(server: &DaemonServer) -> DaemonResponse {
    let godot = server.godot.lock().unwrap();
    if let Some(ref client) = *godot
        && let Some(path) = client.godot_project_path()
    {
        ok_response(serde_json::json!({"path": path}))
    } else {
        error_response("Godot LSP not connected — no project path available")
    }
}

fn dispatch_cached_godot_path(server: &DaemonServer) -> DaemonResponse {
    let guard = server.cached_godot_path.lock().unwrap();
    match guard.as_ref() {
        Some(path) => ok_response(serde_json::json!({"godot_path": path})),
        None => error_response("No Godot path cached — run `gd run` once to cache it"),
    }
}

fn dispatch_cache_godot_path(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(path) = params.get("godot_path").and_then(|p| p.as_str()) else {
        return error_response("missing 'godot_path' parameter");
    };
    *server.cached_godot_path.lock().unwrap() = Some(path.to_string());
    ok_response(serde_json::json!({"cached": true}))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn ok_response(val: serde_json::Value) -> DaemonResponse {
    DaemonResponse {
        result: Some(val),
        error: None,
    }
}

fn error_response(msg: &str) -> DaemonResponse {
    DaemonResponse {
        result: None,
        error: Some(msg.to_string()),
    }
}

// ── Content-Length framed I/O ────────────────────────────────────────────────

fn read_request(reader: &mut impl BufRead) -> Option<DaemonRequest> {
    let content_length = read_content_length(reader)?;
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn write_response(writer: &mut impl Write, response: &DaemonResponse) -> std::io::Result<()> {
    let body = serde_json::to_string(response)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()
}

fn read_content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            let len: usize = len_str.trim().parse().ok()?;
            loop {
                line.clear();
                if reader.read_line(&mut line).ok()? == 0 {
                    return Some(len);
                }
                if line.trim().is_empty() {
                    return Some(len);
                }
            }
        }
    }
}

/// Read the state file from `.godot/gd-daemon.json`.
pub fn read_state_file(project_root: &Path) -> Option<DaemonState> {
    let path = project_root.join(".godot").join("gd-daemon.json");
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_file_roundtrip() {
        let state = DaemonState {
            pid: 12345,
            port: 54321,
            game_pid: None,
            build_id: "0.1.0-123456".to_string(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: DaemonState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pid, 12345);
        assert_eq!(parsed.port, 54321);
    }

    #[test]
    fn test_request_parsing() {
        let json = r#"{"method":"hover","params":{"file":"test.gd","line":10,"column":5}}"#;
        let req: DaemonRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "hover");
        assert_eq!(req.params["file"], "test.gd");
        assert_eq!(req.params["line"], 10);
        assert_eq!(req.params["column"], 5);
    }

    #[test]
    fn test_request_parsing_no_params() {
        let json = r#"{"method":"shutdown"}"#;
        let req: DaemonRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "shutdown");
    }

    #[test]
    fn test_response_serialization() {
        let resp = DaemonResponse {
            result: Some(serde_json::json!({"content": "hello"})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("content"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = error_response("bad request");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("bad request"));
        assert!(!json.contains("result"));
    }

    #[test]
    fn test_content_length_framing() {
        let body = r#"{"method":"shutdown"}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = std::io::BufReader::new(frame.as_bytes());
        let req = read_request(&mut reader).unwrap();
        assert_eq!(req.method, "shutdown");
    }
}
