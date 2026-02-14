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
        "hover" => dispatch_hover(server, &request.params),
        "completion" => dispatch_completion(server, &request.params),
        "definition" => dispatch_definition(server, &request.params),
        // Daemon status
        "status" => dispatch_status(server),
        // Project path (from Godot LSP URI mapping)
        "godot_project_path" => dispatch_godot_project_path(server),
        // Godot binary path cache (WSL probe)
        "cached_godot_path" => dispatch_cached_godot_path(server),
        "cache_godot_path" => dispatch_cache_godot_path(server, &request.params),
        // Binary debug protocol
        "set_game_pid" => dispatch_set_game_pid(server, &request.params),
        "debug_stop_game" => dispatch_debug_stop_game(server),
        "debug_start_server" => dispatch_debug_start_server(server, &request.params),
        "debug_accept" => dispatch_debug_accept(server, &request.params),
        "debug_scene_tree" => dispatch_debug_scene_tree(server),
        "debug_inspect" => dispatch_debug_inspect(server, &request.params),
        "debug_set_property" => dispatch_debug_set_property(server, &request.params),
        "debug_suspend" => dispatch_debug_suspend(server, &request.params),
        "debug_next_frame" => dispatch_debug_next_frame(server),
        "debug_time_scale" => dispatch_debug_time_scale(server, &request.params),
        "debug_reload_scripts" => dispatch_debug_reload_scripts(server, &request.params),
        "debug_server_status" => dispatch_debug_server_status(server),
        "debug_is_at_breakpoint" => dispatch_debug_is_at_breakpoint(server),
        // Core debugger (binary protocol)
        "debug_continue" => dispatch_debug_cmd_simple(server, "continue"),
        "debug_break_exec" => dispatch_debug_cmd_simple(server, "break"),
        "debug_next_step" => dispatch_debug_cmd_simple(server, "next"),
        "debug_step_in" => dispatch_debug_cmd_simple(server, "step"),
        "debug_step_out" => dispatch_debug_cmd_simple(server, "out"),
        "debug_breakpoint" => dispatch_debug_breakpoint(server, &request.params),
        "debug_set_skip_breakpoints" => {
            dispatch_debug_bool_cmd(server, "skip_breakpoints", &request.params)
        }
        "debug_set_ignore_error_breaks" => {
            dispatch_debug_bool_cmd(server, "ignore_error_breaks", &request.params)
        }
        "debug_get_stack_dump" => dispatch_debug_get_stack_dump(server),
        "debug_get_stack_frame_vars" => {
            dispatch_debug_get_stack_frame_vars(server, &request.params)
        }
        "debug_evaluate" => dispatch_debug_evaluate(server, &request.params),
        "debug_reload_all_scripts" => dispatch_debug_simple(server, "reload_all_scripts"),
        // Scene inspection
        "debug_inspect_objects" => dispatch_debug_inspect_objects(server, &request.params),
        "debug_clear_selection" => dispatch_debug_simple(server, "clear_selection"),
        "debug_save_node" => dispatch_debug_save_node(server, &request.params),
        "debug_set_property_field" => dispatch_debug_set_property_field(server, &request.params),
        // Audio
        "debug_mute_audio" => dispatch_debug_mute_audio(server, &request.params),
        // File reload
        "debug_reload_cached_files" => dispatch_debug_reload_cached_files(server, &request.params),
        // Camera override
        "debug_override_cameras" => dispatch_debug_override_cameras(server, &request.params),
        "debug_transform_camera_2d" => dispatch_debug_transform_camera_2d(server, &request.params),
        "debug_transform_camera_3d" => dispatch_debug_transform_camera_3d(server, &request.params),
        // Screenshots
        "debug_request_screenshot" => dispatch_debug_request_screenshot(server, &request.params),
        // Runtime node selection
        "debug_node_select_set_type" => {
            dispatch_debug_int_cmd(server, "node_select_type", &request.params, "type")
        }
        "debug_node_select_set_mode" => {
            dispatch_debug_int_cmd(server, "node_select_mode", &request.params, "mode")
        }
        "debug_node_select_set_visible" => {
            dispatch_debug_bool_param(server, "node_select_visible", &request.params, "visible")
        }
        "debug_node_select_set_avoid_locked" => {
            dispatch_debug_bool_param(server, "node_select_avoid_locked", &request.params, "avoid")
        }
        "debug_node_select_set_prefer_group" => dispatch_debug_bool_param(
            server,
            "node_select_prefer_group",
            &request.params,
            "prefer",
        ),
        "debug_node_select_reset_camera_2d" => {
            dispatch_debug_simple(server, "node_select_reset_2d")
        }
        "debug_node_select_reset_camera_3d" => {
            dispatch_debug_simple(server, "node_select_reset_3d")
        }
        // Live editing
        "debug_live_set_root" => dispatch_debug_live_set_root(server, &request.params),
        "debug_live_node_path" => dispatch_debug_live_path(server, "node_path", &request.params),
        "debug_live_res_path" => dispatch_debug_live_path(server, "res_path", &request.params),
        "debug_live_node_prop" => dispatch_debug_live_prop(server, "node_prop", &request.params),
        "debug_live_node_prop_res" => {
            dispatch_debug_live_prop_res(server, "node_prop_res", &request.params)
        }
        "debug_live_res_prop" => dispatch_debug_live_prop(server, "res_prop", &request.params),
        "debug_live_res_prop_res" => {
            dispatch_debug_live_prop_res(server, "res_prop_res", &request.params)
        }
        "debug_live_node_call" => dispatch_debug_live_call(server, "node_call", &request.params),
        "debug_live_res_call" => dispatch_debug_live_call(server, "res_call", &request.params),
        "debug_live_create_node" => dispatch_debug_live_create_node(server, &request.params),
        "debug_live_instantiate_node" => {
            dispatch_debug_live_instantiate_node(server, &request.params)
        }
        "debug_live_remove_node" => {
            dispatch_debug_live_single_path(server, "remove_node", &request.params)
        }
        "debug_live_remove_and_keep_node" => {
            dispatch_debug_live_remove_and_keep(server, &request.params)
        }
        "debug_live_restore_node" => dispatch_debug_live_restore_node(server, &request.params),
        "debug_live_duplicate_node" => dispatch_debug_live_duplicate_node(server, &request.params),
        "debug_live_reparent_node" => dispatch_debug_live_reparent_node(server, &request.params),
        // Profiler
        "debug_toggle_profiler" => dispatch_debug_toggle_profiler(server, &request.params),
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

// ── LSP dispatch ─────────────────────────────────────────────────────────────

fn dispatch_hover(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(file) = params.get("file").and_then(|f| f.as_str()) else {
        return error_response("missing 'file' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'column' parameter");
    };

    let godot = server.godot.lock().unwrap();
    match super::query::query_hover(file, line as usize, column as usize, godot.as_ref()) {
        Ok(output) => match serde_json::to_value(&output) {
            Ok(val) => DaemonResponse {
                result: Some(val),
                error: None,
            },
            Err(e) => error_response(&format!("serialize error: {e}")),
        },
        Err(e) => error_response(&format!("{e}")),
    }
}

fn dispatch_completion(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(file) = params.get("file").and_then(|f| f.as_str()) else {
        return error_response("missing 'file' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'column' parameter");
    };

    let godot = server.godot.lock().unwrap();
    match super::query::query_completions(file, line as usize, column as usize, godot.as_ref()) {
        Ok(output) => match serde_json::to_value(&output) {
            Ok(val) => DaemonResponse {
                result: Some(val),
                error: None,
            },
            Err(e) => error_response(&format!("serialize error: {e}")),
        },
        Err(e) => error_response(&format!("{e}")),
    }
}

fn dispatch_definition(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(file) = params.get("file").and_then(|f| f.as_str()) else {
        return error_response("missing 'file' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'column' parameter");
    };

    let godot = server.godot.lock().unwrap();
    match super::query::query_definition(file, line as usize, column as usize, godot.as_ref()) {
        Ok(output) => match serde_json::to_value(&output) {
            Ok(val) => DaemonResponse {
                result: Some(val),
                error: None,
            },
            Err(e) => error_response(&format!("serialize error: {e}")),
        },
        Err(e) => error_response(&format!("{e}")),
    }
}

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

// ── Godot binary path cache ──────────────────────────────────────────────────













fn dispatch_cached_godot_path(server: &DaemonServer) -> DaemonResponse {
    let guard = server.cached_godot_path.lock().unwrap();
    match guard.as_ref() {
        Some(path) => ok_response(serde_json::json!({"godot_path": path})),
        None => error_response("No Godot path cached — run `gd run` once to cache it"),
    }
}

fn dispatch_cache_godot_path(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("godot_path").and_then(|p| p.as_str()) else {
        return error_response("missing 'godot_path' parameter");
    };
    *server.cached_godot_path.lock().unwrap() = Some(path.to_string());
    ok_response(serde_json::json!({"cached": true}))
}

// ── Game process management ──────────────────────────────────────────────────

fn dispatch_set_game_pid(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(pid) = params.get("pid").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'pid' parameter");
    };
    *server.game_pid.lock().unwrap() = Some(pid as u32);
    update_game_pid_in_state(&server.project_root, Some(pid as u32));
    ok_response(serde_json::json!({"pid": pid}))
}

fn dispatch_debug_stop_game(server: &DaemonServer) -> DaemonResponse {
    let pid = server.game_pid.lock().unwrap().take();
    let Some(pid) = pid else {
        return error_response("No game process tracked — was the game launched with `gd run`?");
    };

    crate::cli::stop_cmd::kill_game_process(pid);

    // Clear debug server connection, game_running flag, and persisted PID
    *server.debug_server.lock().unwrap() = None;
    server
        .game_running
        .store(false, std::sync::atomic::Ordering::Release);
    update_game_pid_in_state(&server.project_root, None);

    ok_response(serde_json::json!({"stopped": true, "pid": pid}))
}

// ── Binary debug protocol dispatch ───────────────────────────────────────────

fn dispatch_debug_start_server(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let port = params
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(crate::debug::godot_debug_server::GodotDebugServer::DEFAULT_PORT as u64)
        as u16;

    // Reuse existing server if it's on the same port
    {
        let guard = server.debug_server.lock().unwrap();
        if let Some(existing) = guard.as_ref()
            && existing.port() == port
        {
            return ok_response(serde_json::json!({"port": port}));
        }
    }

    // Drop existing server first to release the old port
    *server.debug_server.lock().unwrap() = None;
    // Brief pause to let the OS release the port if needed
    std::thread::sleep(Duration::from_millis(50));

    let Some(ds) = crate::debug::godot_debug_server::GodotDebugServer::new(port) else {
        return error_response("Failed to create debug server (port may be in use)");
    };
    let actual_port = ds.port();
    *server.debug_server.lock().unwrap() = Some(Arc::new(ds));
    ok_response(serde_json::json!({"port": actual_port}))
}

fn dispatch_debug_accept(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let timeout = params.get("timeout").and_then(serde_json::Value::as_u64).unwrap_or(30);

    // Clone the Arc so we can release the mutex before blocking on accept.
    // This is critical — accept() can block for up to 30s and we must not
    // hold the lock during that time or all debug queries will time out.
    let ds = {
        let guard = server.debug_server.lock().unwrap();
        match guard.as_ref() {
            Some(ds) => Arc::clone(ds),
            None => {
                return error_response("No debug server running — call debug_start_server first");
            }
        }
    };

    let connected = ds.accept(Duration::from_secs(timeout));
    if connected {
        server
            .game_running
            .store(true, std::sync::atomic::Ordering::Release);
    }
    ok_response(serde_json::json!({"connected": connected}))
}

/// Clone the debug server Arc from the daemon mutex.
/// This releases the mutex immediately so other daemon queries aren't blocked
/// while long-running debug commands (batch inspect, accept, etc.) execute.
fn get_debug_server(
    server: &DaemonServer,
) -> Option<Arc<crate::debug::godot_debug_server::GodotDebugServer>> {
    server.debug_server.lock().unwrap().as_ref().map(Arc::clone)
}

fn dispatch_debug_scene_tree(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_request_scene_tree() {
        Some(tree) => ok_response(serde_json::to_value(&tree).unwrap_or_default()),
        None => error_response("scene tree request failed or timed out"),
    }
}

fn dispatch_debug_inspect(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_inspect_object(object_id) {
        Some(info) => ok_response(serde_json::to_value(&info).unwrap_or_default()),
        None => error_response(&format!(
            "object {object_id} not found — it may have been freed (try refreshing the scene tree)"
        )),
    }
}

fn dispatch_debug_set_property(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let value_param = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let variant = json_to_variant(&value_param);

    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_set_object_property(object_id, property, variant) {
        ok_response(serde_json::json!({"set": true}))
    } else {
        error_response("set_object_property failed")
    }
}

fn dispatch_debug_suspend(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let suspend = params
        .get("suspend")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_suspend(suspend) {
        ok_response(serde_json::json!({"suspended": suspend}))
    } else {
        error_response("suspend command failed")
    }
}

fn dispatch_debug_next_frame(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_next_frame() {
        ok_response(serde_json::json!({"advanced": true}))
    } else {
        error_response("next_frame command failed")
    }
}

fn dispatch_debug_time_scale(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(scale) = params.get("scale").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'scale' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_set_speed(scale) {
        ok_response(serde_json::json!({"scale": scale}))
    } else {
        error_response("set_speed command failed")
    }
}

fn dispatch_debug_reload_scripts(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let paths: Vec<String> = params
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if paths.is_empty() {
        // No specific paths → reload all scripts unconditionally
        if ds.cmd_reload_all_scripts() {
            ok_response(serde_json::json!({"reloaded": true, "mode": "all"}))
        } else {
            error_response("reload_all_scripts command failed")
        }
    } else if ds.cmd_reload_scripts(&paths) {
        ok_response(serde_json::json!({"reloaded": true, "mode": "selective", "paths": paths}))
    } else {
        error_response("reload_scripts command failed")
    }
}

fn dispatch_debug_server_status(server: &DaemonServer) -> DaemonResponse {
    match get_debug_server(server) {
        Some(ds) => ok_response(serde_json::json!({
            "running": true,
            "port": ds.port(),
            "connected": ds.is_connected(),
        })),
        None => ok_response(serde_json::json!({"running": false})),
    }
}

/// Fast breakpoint state check — reads an atomic flag, no network round-trip.
fn dispatch_debug_is_at_breakpoint(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    ok_response(serde_json::json!({"at_breakpoint": ds.is_at_breakpoint()}))
}

/// Simple execution control command (continue/break/next/step/out).
fn dispatch_debug_cmd_simple(server: &DaemonServer, action: &str) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match action {
        "continue" => ds.cmd_continue(),
        "break" => ds.cmd_break(),
        "next" => ds.cmd_next(),
        "step" => ds.cmd_step(),
        "out" => ds.cmd_out(),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{action} failed"))
    }
}

/// Simple no-arg command that returns success/failure.
fn dispatch_debug_simple(server: &DaemonServer, label: &str) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "reload_all_scripts" => ds.cmd_reload_all_scripts(),
        "clear_selection" => ds.cmd_clear_selection(),
        "node_select_reset_2d" => ds.cmd_runtime_node_select_reset_camera_2d(),
        "node_select_reset_3d" => ds.cmd_runtime_node_select_reset_camera_3d(),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

fn dispatch_debug_breakpoint(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let enabled = params
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_breakpoint(path, line as u32, enabled) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("breakpoint command failed")
    }
}

/// Boolean command helper — dispatches to set_skip_breakpoints or set_ignore_error_breaks.
fn dispatch_debug_bool_cmd(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(value) = params.get("value").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'value' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "skip_breakpoints" => ds.cmd_set_skip_breakpoints(value),
        "ignore_error_breaks" => ds.cmd_set_ignore_error_breaks(value),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

fn dispatch_debug_get_stack_dump(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_get_stack_dump() {
        Some(frames) => ok_response(serde_json::to_value(&frames).unwrap_or_default()),
        None => error_response("get_stack_dump failed or timed out"),
    }
}

fn dispatch_debug_get_stack_frame_vars(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(frame) = params.get("frame").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'frame' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_get_stack_frame_vars(frame as u32) {
        Some(vars) => ok_response(serde_json::to_value(&vars).unwrap_or_default()),
        None => error_response("get_stack_frame_vars failed or timed out"),
    }
}

fn dispatch_debug_evaluate(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(expression) = params.get("expression").and_then(|e| e.as_str()) else {
        return error_response("missing 'expression' parameter");
    };
    let frame = params.get("frame").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_evaluate(expression, frame as u32) {
        Some(result) => ok_response(serde_json::to_value(&result).unwrap_or_default()),
        None => error_response("evaluate failed or timed out"),
    }
}

fn dispatch_debug_inspect_objects(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(ids_arr) = params.get("ids").and_then(|i| i.as_array()) else {
        return error_response("missing 'ids' parameter");
    };
    let ids: Vec<u64> = ids_arr.iter().filter_map(serde_json::Value::as_u64).collect();
    let selection = params
        .get("selection")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_inspect_objects(&ids, selection) {
        Some(results) => ok_response(serde_json::to_value(&results).unwrap_or_default()),
        None => error_response("inspect_objects failed"),
    }
}

fn dispatch_debug_save_node(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_save_node(object_id, path) {
        Some(saved_path) => ok_response(serde_json::json!({"saved": saved_path})),
        None => error_response("save_node failed"),
    }
}

fn dispatch_debug_set_property_field(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let Some(field) = params.get("field").and_then(|f| f.as_str()) else {
        return error_response("missing 'field' parameter");
    };
    let value_param = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let new_field_val = json_to_variant(&value_param);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };

    // Godot's fieldwise_assign casts the value to the property's type, so passing a
    // scalar (e.g. Float(7.0)) for a Vector3 sub-field silently fails.
    // Fix: inspect → modify sub-field client-side → set the full property value.
    let Some(info) = ds.cmd_inspect_object(object_id) else {
        return error_response("failed to inspect object — is the game running?");
    };
    let Some(prop) = info.properties.iter().find(|p| p.name == property) else {
        return error_response(&format!(
            "property '{property}' not found on object {object_id}"
        ));
    };
    let mut current = prop.value.clone();
    if !variant_set_field(&mut current, field, &new_field_val) {
        return error_response(&format!("cannot set field '{field}' on {current:?}"));
    }
    if ds.cmd_set_object_property(object_id, property, current) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("set_object_property failed")
    }
}

/// Set a named sub-field on a GodotVariant (client-side fieldwise assignment).
#[allow(clippy::too_many_lines)]
fn variant_set_field(
    target: &mut crate::debug::variant::GodotVariant,
    field: &str,
    value: &crate::debug::variant::GodotVariant,
) -> bool {
    use crate::debug::variant::GodotVariant;

    let as_f64 = match value {
        GodotVariant::Float(f) => Some(*f),
        GodotVariant::Int(i) => Some(*i as f64),
        _ => None,
    };
    let as_f32 = as_f64.map(|f| f as f32);
    let as_i32 = match value {
        GodotVariant::Int(i) => Some(*i as i32),
        GodotVariant::Float(f) => Some(*f as i32),
        _ => None,
    };

    match target {
        GodotVariant::Vector2(x, y) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                _ => return false,
            }
        }
        GodotVariant::Vector2i(x, y) => {
            let Some(v) = as_i32 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                _ => return false,
            }
        }
        GodotVariant::Vector3(x, y, z) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                _ => return false,
            }
        }
        GodotVariant::Vector3i(x, y, z) => {
            let Some(v) = as_i32 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                _ => return false,
            }
        }
        GodotVariant::Vector4(x, y, z, w) | GodotVariant::Quaternion(x, y, z, w) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                "w" => *w = v,
                _ => return false,
            }
        }
        GodotVariant::Vector4i(x, y, z, w) => {
            let Some(v) = as_i32 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "z" => *z = v,
                "w" => *w = v,
                _ => return false,
            }
        }
        GodotVariant::Color(r, g, b, a) => {
            let Some(v) = as_f32 else { return false };
            match field {
                "r" => *r = v,
                "g" => *g = v,
                "b" => *b = v,
                "a" => *a = v,
                _ => return false,
            }
        }
        GodotVariant::Rect2(x, y, w, h) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *x = v,
                "y" => *y = v,
                "w" | "width" => *w = v,
                "h" | "height" => *h = v,
                _ => return false,
            }
        }
        GodotVariant::Plane(a, b, c, d) => {
            let Some(v) = as_f64 else { return false };
            match field {
                "x" => *a = v,
                "y" => *b = v,
                "z" => *c = v,
                "d" => *d = v,
                _ => return false,
            }
        }
        _ => return false,
    }
    true
}

fn dispatch_debug_mute_audio(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(mute) = params.get("mute").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'mute' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_mute_audio(mute) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("mute_audio failed")
    }
}

fn dispatch_debug_reload_cached_files(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(files_arr) = params.get("files").and_then(|f| f.as_array()) else {
        return error_response("missing 'files' parameter");
    };
    let files: Vec<&str> = files_arr.iter().filter_map(|v| v.as_str()).collect();
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_reload_cached_files(&files) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("reload_cached_files failed")
    }
}

fn dispatch_debug_override_cameras(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(enable) = params.get("enable").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'enable' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_override_cameras(enable) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("override_cameras failed")
    }
}

fn dispatch_debug_transform_camera_2d(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(arr) = params.get("transform").and_then(|t| t.as_array()) else {
        return error_response("missing 'transform' parameter");
    };
    if arr.len() != 6 {
        return error_response("'transform' must have 6 elements");
    }
    let mut transform = [0.0f64; 6];
    for (i, v) in arr.iter().enumerate() {
        transform[i] = v.as_f64().unwrap_or(0.0);
    }
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_transform_camera_2d(transform) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("transform_camera_2d failed")
    }
}

fn dispatch_debug_transform_camera_3d(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(arr) = params.get("transform").and_then(|t| t.as_array()) else {
        return error_response("missing 'transform' parameter");
    };
    if arr.len() != 12 {
        return error_response("'transform' must have 12 elements");
    }
    let mut transform = [0.0f64; 12];
    for (i, v) in arr.iter().enumerate() {
        transform[i] = v.as_f64().unwrap_or(0.0);
    }
    let perspective = params
        .get("perspective")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let Some(fov) = params.get("fov").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'fov' parameter");
    };
    let Some(near) = params.get("near").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'near' parameter");
    };
    let Some(far) = params.get("far").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'far' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_transform_camera_3d(transform, perspective, fov, near, far) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("transform_camera_3d failed")
    }
}

fn dispatch_debug_request_screenshot(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_request_screenshot(id) {
        Some(result) => {
            // Read the PNG file from Godot's temp dir and base64-encode it
            let file_path = crate::core::fs::windows_to_wsl_path(&result.path);
            match std::fs::read(&file_path) {
                Ok(bytes) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    // Clean up the temp file
                    let _ = std::fs::remove_file(&file_path);
                    ok_response(serde_json::json!({
                        "width": result.width,
                        "height": result.height,
                        "data": b64,
                        "format": "png",
                    }))
                }
                Err(e) => error_response(&format!(
                    "Screenshot captured ({}x{}) but failed to read {}: {e}",
                    result.width, result.height, file_path
                )),
            }
        }
        None => error_response("request_screenshot failed or timed out"),
    }
}

/// Integer parameter command helper (node selection type/mode).
fn dispatch_debug_int_cmd(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
    param_name: &str,
) -> DaemonResponse {
    let Some(value) = params.get(param_name).and_then(serde_json::Value::as_i64) else {
        return error_response(&format!("missing '{param_name}' parameter"));
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_select_type" => ds.cmd_runtime_node_select_set_type(value as i32),
        "node_select_mode" => ds.cmd_runtime_node_select_set_mode(value as i32),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

/// Boolean parameter command helper (node selection bools).
fn dispatch_debug_bool_param(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
    param_name: &str,
) -> DaemonResponse {
    let Some(value) = params.get(param_name).and_then(serde_json::Value::as_bool) else {
        return error_response(&format!("missing '{param_name}' parameter"));
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_select_visible" => ds.cmd_runtime_node_select_set_visible(value),
        "node_select_avoid_locked" => ds.cmd_runtime_node_select_set_avoid_locked(value),
        "node_select_prefer_group" => ds.cmd_runtime_node_select_set_prefer_group(value),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

// ── Live editing dispatch ────────────────────────────────────────────────────

fn dispatch_debug_live_set_root(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(scene_path) = params.get("scene_path").and_then(|s| s.as_str()) else {
        return error_response("missing 'scene_path' parameter");
    };
    let Some(scene_file) = params.get("scene_file").and_then(|s| s.as_str()) else {
        return error_response("missing 'scene_file' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_set_root(scene_path, scene_file) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_set_root failed")
    }
}

/// Shared helper for live_node_path / live_res_path.
fn dispatch_debug_live_path(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_path" => ds.cmd_live_node_path(path, id as i32),
        "res_path" => ds.cmd_live_res_path(path, id as i32),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

/// Shared helper for live_node_prop / live_res_prop.
fn dispatch_debug_live_prop(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let value_param = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let variant = json_to_variant(&value_param);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_prop" => ds.cmd_live_node_prop(id as i32, property, variant),
        "res_prop" => ds.cmd_live_res_prop(id as i32, property, variant),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

/// Shared helper for live_node_prop_res / live_res_prop_res.
fn dispatch_debug_live_prop_res(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let Some(res_path) = params.get("res_path").and_then(|r| r.as_str()) else {
        return error_response("missing 'res_path' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_prop_res" => ds.cmd_live_node_prop_res(id as i32, property, res_path),
        "res_prop_res" => ds.cmd_live_res_prop_res(id as i32, property, res_path),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

/// Shared helper for live_node_call / live_res_call.
fn dispatch_debug_live_call(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(method) = params.get("method").and_then(|m| m.as_str()) else {
        return error_response("missing 'method' parameter");
    };
    let args: Vec<crate::debug::variant::GodotVariant> = params
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().map(json_to_variant).collect())
        .unwrap_or_default();
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_call" => ds.cmd_live_node_call(id as i32, method, &args),
        "res_call" => ds.cmd_live_res_call(id as i32, method, &args),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

fn dispatch_debug_live_create_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(parent) = params.get("parent").and_then(|p| p.as_str()) else {
        return error_response("missing 'parent' parameter");
    };
    let Some(class) = params.get("class").and_then(|c| c.as_str()) else {
        return error_response("missing 'class' parameter");
    };
    let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
        return error_response("missing 'name' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_create_node(parent, class, name) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_create_node failed")
    }
}

fn dispatch_debug_live_instantiate_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(parent) = params.get("parent").and_then(|p| p.as_str()) else {
        return error_response("missing 'parent' parameter");
    };
    let Some(scene) = params.get("scene").and_then(|s| s.as_str()) else {
        return error_response("missing 'scene' parameter");
    };
    let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
        return error_response("missing 'name' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_instantiate_node(parent, scene, name) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_instantiate_node failed")
    }
}

/// Single path command helper (remove_node).
fn dispatch_debug_live_single_path(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "remove_node" => ds.cmd_live_remove_node(path),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

fn dispatch_debug_live_remove_and_keep(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_remove_and_keep_node(path, object_id) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_remove_and_keep_node failed")
    }
}

fn dispatch_debug_live_restore_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(pos) = params.get("pos").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'pos' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_restore_node(object_id, path, pos as i32) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_restore_node failed")
    }
}

fn dispatch_debug_live_duplicate_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(new_name) = params.get("new_name").and_then(|n| n.as_str()) else {
        return error_response("missing 'new_name' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_duplicate_node(path, new_name) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_duplicate_node failed")
    }
}

fn dispatch_debug_live_reparent_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(new_parent) = params.get("new_parent").and_then(|n| n.as_str()) else {
        return error_response("missing 'new_parent' parameter");
    };
    let Some(new_name) = params.get("new_name").and_then(|n| n.as_str()) else {
        return error_response("missing 'new_name' parameter");
    };
    let Some(pos) = params.get("pos").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'pos' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_reparent_node(path, new_parent, new_name, pos as i32) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_reparent_node failed")
    }
}

fn dispatch_debug_toggle_profiler(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(profiler) = params.get("profiler").and_then(|p| p.as_str()) else {
        return error_response("missing 'profiler' parameter");
    };
    let Some(enable) = params.get("enable").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'enable' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_toggle_profiler(profiler, enable) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("toggle_profiler failed")
    }
}

fn json_to_variant(value: &serde_json::Value) -> crate::debug::variant::GodotVariant {
    use crate::debug::variant::GodotVariant;
    match value {
        serde_json::Value::Null => GodotVariant::Nil,
        serde_json::Value::Bool(b) => GodotVariant::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                GodotVariant::Int(i)
            } else if let Some(f) = n.as_f64() {
                GodotVariant::Float(f)
            } else {
                GodotVariant::Nil
            }
        }
        serde_json::Value::String(s) => GodotVariant::String(s.clone()),
        serde_json::Value::Array(arr) => json_array_to_variant(arr),
        serde_json::Value::Object(obj) => json_object_to_variant(obj),
    }
}

/// Convert a JSON array to the best-fit GodotVariant based on element count.
/// Float arrays: 2→Vector2, 3→Vector3, 4→Vector4, 6→Transform2D, 9→Basis, 12→Transform3D, 16→Projection
/// Int arrays (all integers): 2→Vector2i, 3→Vector3i, 4→Vector4i
/// Mixed/other: generic Array with recursive conversion.
fn json_array_to_variant(arr: &[serde_json::Value]) -> crate::debug::variant::GodotVariant {
    use crate::debug::variant::GodotVariant;

    // Check if all elements are numbers
    let all_numbers = arr.iter().all(serde_json::Value::is_number);
    if !all_numbers {
        // Generic array — recurse into each element
        return GodotVariant::Array(arr.iter().map(json_to_variant).collect());
    }

    // Check if all elements are integers (no fractional part)
    let all_ints = arr.iter().all(|v| v.as_i64().is_some());

    let floats: Vec<f64> = arr.iter().filter_map(serde_json::Value::as_f64).collect();
    if floats.len() != arr.len() {
        return GodotVariant::Array(arr.iter().map(json_to_variant).collect());
    }

    match floats.len() {
        2 if all_ints => GodotVariant::Vector2i(floats[0] as i32, floats[1] as i32),
        2 => GodotVariant::Vector2(floats[0], floats[1]),
        3 if all_ints => {
            GodotVariant::Vector3i(floats[0] as i32, floats[1] as i32, floats[2] as i32)
        }
        3 => GodotVariant::Vector3(floats[0], floats[1], floats[2]),
        4 if all_ints => GodotVariant::Vector4i(
            floats[0] as i32,
            floats[1] as i32,
            floats[2] as i32,
            floats[3] as i32,
        ),
        4 => GodotVariant::Vector4(floats[0], floats[1], floats[2], floats[3]),
        6 => GodotVariant::Transform2D([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
        ]),
        9 => GodotVariant::Basis([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5], floats[6], floats[7],
            floats[8],
        ]),
        12 => GodotVariant::Transform3D([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5], floats[6], floats[7],
            floats[8], floats[9], floats[10], floats[11],
        ]),
        16 => GodotVariant::Projection([
            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5], floats[6], floats[7],
            floats[8], floats[9], floats[10], floats[11], floats[12], floats[13], floats[14],
            floats[15],
        ]),
        _ => GodotVariant::Array(arr.iter().map(json_to_variant).collect()),
    }
}

/// Convert a JSON object to a GodotVariant.
/// Supports typed wrappers: `{"Vector3": [1,2,3]}`, `{"Color": [1,0,0,1]}`, etc.
/// Falls back to Dictionary for unrecognized shapes.
fn json_object_to_variant(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> crate::debug::variant::GodotVariant {
    use crate::debug::variant::GodotVariant;

    // Single-key type wrapper: {"Vector3": [1.0, 2.0, 3.0]}
    if obj.len() == 1 {
        let (key, inner) = obj.iter().next().unwrap();
        if let Some(arr) = inner.as_array() {
            let floats: Vec<f64> = arr.iter().filter_map(serde_json::Value::as_f64).collect();
            if floats.len() == arr.len() {
                match (key.as_str(), floats.len()) {
                    ("Vector2", 2) => return GodotVariant::Vector2(floats[0], floats[1]),
                    ("Vector2i", 2) => {
                        return GodotVariant::Vector2i(floats[0] as i32, floats[1] as i32);
                    }
                    ("Rect2", 4) => {
                        return GodotVariant::Rect2(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("Rect2i", 4) => {
                        return GodotVariant::Rect2i(
                            floats[0] as i32,
                            floats[1] as i32,
                            floats[2] as i32,
                            floats[3] as i32,
                        );
                    }
                    ("Vector3", 3) => {
                        return GodotVariant::Vector3(floats[0], floats[1], floats[2]);
                    }
                    ("Vector3i", 3) => {
                        return GodotVariant::Vector3i(
                            floats[0] as i32,
                            floats[1] as i32,
                            floats[2] as i32,
                        );
                    }
                    ("Transform2D", 6) => {
                        return GodotVariant::Transform2D([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                        ]);
                    }
                    ("Vector4", 4) => {
                        return GodotVariant::Vector4(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("Vector4i", 4) => {
                        return GodotVariant::Vector4i(
                            floats[0] as i32,
                            floats[1] as i32,
                            floats[2] as i32,
                            floats[3] as i32,
                        );
                    }
                    ("Plane", 4) => {
                        return GodotVariant::Plane(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("Quaternion", 4) => {
                        return GodotVariant::Quaternion(floats[0], floats[1], floats[2], floats[3]);
                    }
                    ("AABB", 6) => {
                        return GodotVariant::Aabb([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                        ]);
                    }
                    ("Basis", 9) => {
                        return GodotVariant::Basis([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                            floats[6], floats[7], floats[8],
                        ]);
                    }
                    ("Transform3D", 12) => {
                        return GodotVariant::Transform3D([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                            floats[6], floats[7], floats[8], floats[9], floats[10], floats[11],
                        ]);
                    }
                    ("Projection", 16) => {
                        return GodotVariant::Projection([
                            floats[0], floats[1], floats[2], floats[3], floats[4], floats[5],
                            floats[6], floats[7], floats[8], floats[9], floats[10], floats[11],
                            floats[12], floats[13], floats[14], floats[15],
                        ]);
                    }
                    ("Color", 4) => {
                        return GodotVariant::Color(
                            floats[0] as f32,
                            floats[1] as f32,
                            floats[2] as f32,
                            floats[3] as f32,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    // Generic dictionary
    GodotVariant::Dictionary(
        obj.iter()
            .map(|(k, v)| (GodotVariant::String(k.clone()), json_to_variant(v)))
            .collect(),
    )
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
