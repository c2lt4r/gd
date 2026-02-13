use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use super::godot_client::GodotClient;
use super::workspace::WorkspaceIndex;
use crate::debug::dap_client::DapClient;

/// State file written to `.godot/gd-daemon.json` so CLI clients can find us.
#[derive(Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub port: u16,
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
    dap: Mutex<Option<Arc<DapClient>>>,
    dap_caps: Mutex<Option<serde_json::Value>>,
    /// True when a game was launched via DAP and hasn't exited yet.
    game_running: std::sync::atomic::AtomicBool,
    workspace: WorkspaceIndex,
    project_root: PathBuf,
    godot_port: u16,
    dap_host: String,
    dap_port: u16,
    last_activity: Mutex<Instant>,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Entry point for `gd lsp daemon`. Runs a persistent background server.
pub fn run(
    project_root: PathBuf,
    godot_port: u16,
    dap_host: String,
    dap_port: u16,
) -> miette::Result<()> {
    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| miette::miette!("cannot bind TCP listener: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| miette::miette!("cannot get local address: {e}"))?
        .port();

    // Build workspace index for cross-file resolution
    let workspace = WorkspaceIndex::new(project_root.clone());

    let server = std::sync::Arc::new(DaemonServer {
        godot: Mutex::new(None),
        godot_ready: std::sync::atomic::AtomicBool::new(false),
        dap: Mutex::new(None),
        dap_caps: Mutex::new(None),
        game_running: std::sync::atomic::AtomicBool::new(false),
        workspace,
        project_root: project_root.clone(),
        godot_port,
        dap_host,
        dap_port,
        last_activity: Mutex::new(Instant::now()),
    });

    // Write state file immediately so clients can connect
    let state_path = project_root.join(".godot").join("gd-daemon.json");
    write_state_file(&state_path, port)?;

    // Connect to Godot LSP + DAP in a background thread (handshake can be slow)
    {
        let server_init = std::sync::Arc::clone(&server);
        let root = project_root.clone();
        std::thread::spawn(move || {
            // Connect to Godot's LSP
            if godot_port > 0
                && let Some(client) = GodotClient::connect("127.0.0.1", godot_port)
            {
                client.initialize(&root);
                *server_init.godot.lock().unwrap() = Some(client);
            }
            server_init
                .godot_ready
                .store(true, std::sync::atomic::Ordering::Release);

            // Connect to Godot's DAP
            let (dap, caps) = try_connect_dap(&server_init.dap_host, server_init.dap_port);
            if dap.is_some() {
                *server_init.dap.lock().unwrap() = dap;
                *server_init.dap_caps.lock().unwrap() = caps;
            }
        });
    }

    // Spawn idle monitor thread
    let state_path_clone = state_path.clone();
    let server_idle = std::sync::Arc::clone(&server);
    std::thread::spawn(move || idle_monitor(server_idle, &state_path_clone));

    // Accept connections — spawn threads for blocking requests
    for stream in listener.incoming() {
        match stream {
            Ok(conn) => {
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
            Err(_) => continue,
        }
    }

    Ok(())
}

fn try_connect_dap(host: &str, port: u16) -> (Option<Arc<DapClient>>, Option<serde_json::Value>) {
    if port == 0 {
        return (None, None);
    }
    let Some(client) = DapClient::connect(host, port) else {
        return (None, None);
    };
    let caps = client.handshake();
    (Some(Arc::new(client)), caps)
}

fn write_state_file(path: &Path, port: u16) -> miette::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| miette::miette!("cannot create state directory: {e}"))?;
    }
    let state = DaemonState {
        pid: std::process::id(),
        port,
    };
    let json =
        serde_json::to_string_pretty(&state).map_err(|e| miette::miette!("serialize: {e}"))?;
    std::fs::write(path, json).map_err(|e| miette::miette!("cannot write state file: {e}"))?;
    Ok(())
}

fn idle_monitor(server: std::sync::Arc<DaemonServer>, state_path: &Path) {
    loop {
        std::thread::sleep(IDLE_CHECK_INTERVAL);
        // Never exit while a game is running via DAP
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
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .ok()?;

    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let request = read_request(&mut reader)?;

    // dap_wait_stopped / dap_wait_exited / dap_launch can block — extend timeout
    if request.method == "dap_wait_stopped" || request.method == "dap_wait_exited" || request.method == "dap_launch" {
        let timeout = request
            .params
            .get("timeout")
            .and_then(|t| t.as_u64())
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
        // DAP queries
        "dap_status" => dispatch_dap_status(server),
        "dap_project_path" => dispatch_dap_project_path(server),
        "dap_set_breakpoints" => dispatch_dap_set_breakpoints(server, &request.params),
        "dap_continue" => dispatch_dap_simple(server, "continue"),
        "dap_pause" => dispatch_dap_simple(server, "pause"),
        "dap_next" => dispatch_dap_simple(server, "next"),
        "dap_step_in" => dispatch_dap_simple(server, "step_in"),
        "dap_threads" => dispatch_dap_threads(server),
        "dap_stack_trace" => dispatch_dap_stack_trace(server, &request.params),
        "dap_scopes" => dispatch_dap_scopes(server, &request.params),
        "dap_variables" => dispatch_dap_variables(server, &request.params),
        "dap_evaluate" => dispatch_dap_evaluate(server, &request.params),
        "dap_wait_stopped" => dispatch_dap_wait_stopped(server, &request.params),
        "dap_launch" => dispatch_dap_launch(server, &request.params),
        "dap_wait_exited" => dispatch_dap_wait_exited(server, &request.params),
        "dap_terminate" => dispatch_dap_terminate(server),
        "dap_disconnect" => dispatch_dap_disconnect(server),
        "dap_reconnect" => dispatch_dap_reconnect(server),
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
    let Some(line) = params.get("line").and_then(|l| l.as_u64()) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(|c| c.as_u64()) else {
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
    let Some(line) = params.get("line").and_then(|l| l.as_u64()) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(|c| c.as_u64()) else {
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
    let Some(line) = params.get("line").and_then(|l| l.as_u64()) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(|c| c.as_u64()) else {
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
    let dap_connected = server.dap.lock().unwrap().is_some();
    let godot_path = server
        .godot
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| c.godot_project_path());
    let game_running = server
        .game_running
        .load(std::sync::atomic::Ordering::Acquire);
    ok_response(serde_json::json!({
        "godot_connected": godot_connected,
        "godot_ready": godot_ready,
        "godot_project_path": godot_path,
        "dap_connected": dap_connected,
        "game_running": game_running,
        "project_root": server.project_root.to_string_lossy(),
        "godot_port": server.godot_port,
        "dap_port": server.dap_port,
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

// ── DAP dispatch ─────────────────────────────────────────────────────────────

/// Ensure DAP is connected, reconnecting if needed. Returns true if connected.
fn ensure_dap(server: &DaemonServer) -> bool {
    let mut dap = server.dap.lock().unwrap();
    if dap.is_some() {
        return true;
    }
    // Try reconnecting
    let (new_dap, caps) = try_connect_dap(&server.dap_host, server.dap_port);
    if new_dap.is_some() {
        *dap = new_dap;
        *server.dap_caps.lock().unwrap() = caps;
        true
    } else {
        false
    }
}

fn dispatch_dap_status(server: &DaemonServer) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected — is Godot editor running?");
    }
    let dap = server.dap.lock().unwrap();
    let client = dap.as_ref().unwrap();
    let caps = server.dap_caps.lock().unwrap().clone();
    let threads = client.threads();
    let project_path = client.project_path();

    ok_response(serde_json::json!({
        "connected": true,
        "capabilities": caps,
        "threads": threads.map(|t| t["threads"].clone()),
        "project_path": project_path,
    }))
}

fn dispatch_dap_project_path(server: &DaemonServer) -> DaemonResponse {
    // Prefer Godot LSP project path (always available, doesn't need the DAP stream)
    if let Some(path) = server
        .godot
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| c.godot_project_path())
    {
        return ok_response(serde_json::json!({"project_path": path}));
    }
    // Fallback: DAP client's discovered path
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let dap = server.dap.lock().unwrap();
    let path = dap.as_ref().unwrap().project_path();
    ok_response(serde_json::json!({"project_path": path}))
}

fn dispatch_dap_set_breakpoints(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(lines_arr) = params.get("lines").and_then(|l| l.as_array()) else {
        return error_response("missing 'lines' parameter");
    };
    let lines: Vec<u32> = lines_arr
        .iter()
        .filter_map(|l| l.as_u64().map(|n| n as u32))
        .collect();

    let dap = server.dap.lock().unwrap();
    match dap.as_ref().unwrap().set_breakpoints(path, &lines) {
        Some(body) => ok_response(body),
        None => error_response("setBreakpoints failed"),
    }
}

fn dispatch_dap_simple(server: &DaemonServer, action: &str) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let dap = server.dap.lock().unwrap();
    let client = dap.as_ref().unwrap();
    let thread_id = 1; // Godot uses thread 1

    let result = match action {
        "continue" => client.continue_execution(thread_id),
        "pause" => client.pause(thread_id),
        "next" => client.next(thread_id),
        "step_in" => client.step_in(thread_id),
        _ => None,
    };

    match result {
        Some(body) => ok_response(body),
        None => error_response(&format!("{action} failed")),
    }
}

fn dispatch_dap_threads(server: &DaemonServer) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let dap = server.dap.lock().unwrap();
    match dap.as_ref().unwrap().threads() {
        Some(body) => ok_response(body),
        None => error_response("threads request failed"),
    }
}

fn dispatch_dap_stack_trace(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let thread_id = params
        .get("thread_id")
        .and_then(|t| t.as_i64())
        .unwrap_or(1);
    let dap = server.dap.lock().unwrap();
    match dap.as_ref().unwrap().stack_trace(thread_id) {
        Some(body) => ok_response(body),
        None => error_response("stackTrace failed"),
    }
}

fn dispatch_dap_scopes(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let Some(frame_id) = params.get("frame_id").and_then(|f| f.as_i64()) else {
        return error_response("missing 'frame_id' parameter");
    };
    let dap = server.dap.lock().unwrap();
    match dap.as_ref().unwrap().scopes(frame_id) {
        Some(body) => ok_response(body),
        None => error_response("scopes failed"),
    }
}

fn dispatch_dap_variables(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let Some(vref) = params.get("variables_reference").and_then(|v| v.as_i64()) else {
        return error_response("missing 'variables_reference' parameter");
    };
    let dap = server.dap.lock().unwrap();
    match dap.as_ref().unwrap().variables(vref) {
        Some(body) => ok_response(body),
        None => error_response("variables failed"),
    }
}

fn dispatch_dap_evaluate(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let Some(expression) = params.get("expression").and_then(|e| e.as_str()) else {
        return error_response("missing 'expression' parameter");
    };
    let context = params
        .get("context")
        .and_then(|c| c.as_str())
        .unwrap_or("repl");
    let frame_id = params
        .get("frame_id")
        .and_then(|f| f.as_i64())
        .unwrap_or(0);
    let dap = server.dap.lock().unwrap();
    match dap.as_ref().unwrap().evaluate(expression, context, frame_id) {
        Some(body) => ok_response(body),
        None => error_response("evaluate failed"),
    }
}

fn dispatch_dap_wait_stopped(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    if !ensure_dap(server) {
        return error_response("DAP not connected");
    }
    let timeout = params
        .get("timeout")
        .and_then(|t| t.as_u64())
        .unwrap_or(30);
    // Clone Arc to release mutex before blocking
    let client = {
        let dap = server.dap.lock().unwrap();
        Arc::clone(dap.as_ref().unwrap())
    };
    match client.wait_for_stopped(timeout) {
        Some(body) => ok_response(body),
        None => error_response("timeout waiting for stopped event"),
    }
}

fn dispatch_dap_launch(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    // Guard: don't launch if a game is already running
    if server
        .game_running
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return error_response("Game is already running — terminate it first");
    }

    // Disconnect existing DAP session — launch needs a fresh connection
    {
        let mut dap = server.dap.lock().unwrap();
        if let Some(client) = dap.take() {
            client.disconnect();
        }
    }

    // Connect fresh for launch mode
    let Some(client) = DapClient::connect(&server.dap_host, server.dap_port) else {
        return error_response("DAP not connected — is Godot editor running?");
    };

    // Determine project path: from params, or from Godot LSP, or from our project root.
    // Wait briefly for Godot LSP if it's still initializing (has the correct Windows path).
    let project = params
        .get("project")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            // Wait up to 2s for Godot LSP to be ready (it discovers the project path)
            for _ in 0..20 {
                if server
                    .godot_ready
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            server
                .godot
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|c| c.godot_project_path())
        })
        .unwrap_or_else(|| {
            // Fallback: convert WSL path to Windows path if applicable
            let root = server.project_root.to_string_lossy();
            wsl_to_windows_path(&root).unwrap_or_else(|| root.to_string())
        });

    // Launch — this sends initialize + launch + configurationDone and waits
    // for the process event (which contains the Godot binary path)
    let result = client.launch(&project);

    match result {
        Some(process_body) => {
            *server.dap.lock().unwrap() = Some(Arc::new(client));
            *server.dap_caps.lock().unwrap() = None;
            server
                .game_running
                .store(true, std::sync::atomic::Ordering::Release);
            ok_response(serde_json::json!({
                "launched": true,
                "process": process_body,
            }))
        }
        None => error_response("DAP launch failed"),
    }
}

fn dispatch_dap_wait_exited(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let timeout = params
        .get("timeout")
        .and_then(|t| t.as_u64())
        .unwrap_or(3600); // default 1 hour

    // Clone the Arc so we can release the mutex before blocking
    let client = {
        let dap = server.dap.lock().unwrap();
        match dap.as_ref() {
            Some(c) => Arc::clone(c),
            None => {
                server
                    .game_running
                    .store(false, std::sync::atomic::Ordering::Release);
                return error_response("DAP not connected");
            }
        }
    };

    let result = client.wait_for_exited(timeout);
    server
        .game_running
        .store(false, std::sync::atomic::Ordering::Release);
    match result {
        Some(body) => ok_response(body),
        None => error_response("timeout or connection lost waiting for game to exit"),
    }
}

fn dispatch_dap_disconnect(server: &DaemonServer) -> DaemonResponse {
    let mut dap = server.dap.lock().unwrap();
    if let Some(client) = dap.take() {
        client.disconnect();
    }
    *server.dap_caps.lock().unwrap() = None;
    ok_response(serde_json::json!({"disconnected": true}))
}

fn dispatch_dap_terminate(server: &DaemonServer) -> DaemonResponse {
    let dap = server.dap.lock().unwrap();
    if let Some(client) = dap.as_ref() {
        client.terminate();
    }
    server
        .game_running
        .store(false, std::sync::atomic::Ordering::Release);
    ok_response(serde_json::json!({"terminated": true}))
}

fn dispatch_dap_reconnect(server: &DaemonServer) -> DaemonResponse {
    // Disconnect existing if any
    {
        let mut dap = server.dap.lock().unwrap();
        if let Some(client) = dap.take() {
            client.disconnect();
        }
    }
    // Reconnect
    let (new_dap, caps) = try_connect_dap(&server.dap_host, server.dap_port);
    let connected = new_dap.is_some();
    *server.dap.lock().unwrap() = new_dap;
    *server.dap_caps.lock().unwrap() = caps;
    if connected {
        ok_response(serde_json::json!({"reconnected": true}))
    } else {
        error_response("DAP reconnect failed — is Godot editor running?")
    }
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

/// Convert a WSL path like `/mnt/c/users/carl/project` to `C:/users/carl/project`.
/// Returns None if the path is not a WSL mount path.
fn wsl_to_windows_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/mnt/")?;
    let drive = rest.chars().next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    let remainder = &rest[1..]; // everything after the drive letter (starts with / or is empty)
    Some(format!("{}:{}", drive.to_ascii_uppercase(), remainder))
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

    #[test]
    fn test_dap_request_parsing() {
        let json =
            r#"{"method":"dap_set_breakpoints","params":{"path":"test.gd","lines":[10,20]}}"#;
        let req: DaemonRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "dap_set_breakpoints");
        assert_eq!(req.params["path"], "test.gd");
    }

    #[test]
    fn test_wsl_to_windows_path() {
        assert_eq!(
            wsl_to_windows_path("/mnt/c/projects/game"),
            Some("C:/projects/game".to_string())
        );
        assert_eq!(
            wsl_to_windows_path("/mnt/d/games"),
            Some("D:/games".to_string())
        );
        assert_eq!(wsl_to_windows_path("/mnt/c"), Some("C:".to_string()));
        assert_eq!(wsl_to_windows_path("/home/user/project"), None);
        assert_eq!(wsl_to_windows_path("C:/already/windows"), None);
    }
}
