#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::debug::variant::{GodotVariant, decode_packet, encode_packet};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StackFrameInfo {
    pub file: String,
    pub line: u32,
    pub function: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameVariables {
    pub locals: Vec<DebugVariable>,
    pub members: Vec<DebugVariable>,
    pub globals: Vec<DebugVariable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebugVariable {
    pub name: String,
    pub value: GodotVariant,
    pub var_type: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub name: String,
    pub value: GodotVariant,
    pub var_type: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneTree {
    pub nodes: Vec<SceneNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneNode {
    pub name: String,
    pub class_name: String,
    pub object_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_file_path: Option<String>,
    pub children: Vec<SceneNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScreenshotResult {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectInfo {
    pub object_id: u64,
    pub class_name: String,
    pub properties: Vec<ObjectProperty>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectProperty {
    pub name: String,
    pub value: GodotVariant,
    pub type_id: u32,
    pub hint: u32,
    pub hint_string: String,
    pub usage: u32,
}

// ---------------------------------------------------------------------------
// Inbox — shared between reader thread and main thread
// ---------------------------------------------------------------------------

struct Inbox {
    messages: Mutex<Vec<Vec<GodotVariant>>>,
    notify: Condvar,
}

impl Inbox {
    fn new() -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
            notify: Condvar::new(),
        }
    }

    fn push(&self, msg: Vec<GodotVariant>) {
        let mut msgs = self.messages.lock().unwrap();
        msgs.push(msg);
        self.notify.notify_all();
    }

    /// Wait for a message whose first element is a String matching `prefix`.
    /// Removes and returns it. Returns None on timeout.
    fn wait_for(&self, prefix: &str, timeout: Duration) -> Option<Vec<GodotVariant>> {
        self.wait_for_any(&[prefix], timeout)
    }

    /// Wait for a message matching any of the given prefixes.
    /// Removes and returns the first match. Returns None on timeout.
    fn wait_for_any(&self, prefixes: &[&str], timeout: Duration) -> Option<Vec<GodotVariant>> {
        let deadline = Instant::now() + timeout;
        let mut msgs = self.messages.lock().unwrap();
        loop {
            if let Some(idx) = msgs
                .iter()
                .position(|m| prefixes.iter().any(|p| msg_matches(m, p)))
            {
                return Some(msgs.remove(idx));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (guard, result) = self.notify.wait_timeout(msgs, remaining).unwrap();
            msgs = guard;
            if result.timed_out() {
                if let Some(idx) = msgs
                    .iter()
                    .position(|m| prefixes.iter().any(|p| msg_matches(m, p)))
                {
                    return Some(msgs.remove(idx));
                }
                return None;
            }
        }
    }
}

fn msg_matches(msg: &[GodotVariant], prefix: &str) -> bool {
    matches!(msg.first(), Some(GodotVariant::String(s)) if s == prefix)
}

// ---------------------------------------------------------------------------
// GodotDebugServer
// ---------------------------------------------------------------------------

pub struct GodotDebugServer {
    stream: Mutex<Option<TcpStream>>,
    listener: TcpListener,
    port: u16,
    inbox: Arc<Inbox>,
    /// Set to false when we want the reader thread to stop.
    running: Arc<Mutex<bool>>,
}

impl GodotDebugServer {
    /// Default port for the gd binary debug protocol.
    /// Godot uses 6005 (LSP) and 6006 (DAP), so we use 6008.
    pub const DEFAULT_PORT: u16 = 6008;

    /// Create a new server listening on the given port on all interfaces.
    /// Binds to 0.0.0.0 so the port is reachable from Windows when running in WSL2.
    /// Pass 0 to let the OS assign a random port (useful for tests).
    pub fn new(port: u16) -> Option<Self> {
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).ok()?;
        let port = listener.local_addr().ok()?.port();
        Some(Self {
            stream: Mutex::new(None),
            listener,
            port,
            inbox: Arc::new(Inbox::new()),
            running: Arc::new(Mutex::new(true)),
        })
    }

    /// Get the port we're listening on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Accept a connection from the game (blocking, with timeout).
    /// Returns true if a connection was accepted.
    pub fn accept(&self, timeout: Duration) -> bool {
        let _ = self.listener.set_nonblocking(true);
        let deadline = Instant::now() + timeout;
        loop {
            match self.listener.accept() {
                Ok((tcp_stream, _addr)) => {
                    let _ = tcp_stream.set_nonblocking(false);
                    // Clone for the reader thread
                    let reader_stream = match tcp_stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => return false,
                    };
                    *self.stream.lock().unwrap() = Some(tcp_stream);
                    self.spawn_reader(reader_stream);
                    return true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return false,
            }
        }
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.stream.lock().unwrap().is_some()
    }

    /// Send a command to the game.
    /// Wire format: Array([String(command), Int(thread_id), Array([args...])])
    /// Godot 4.2+ requires three elements: command name, thread_id, and a
    /// data Array wrapping all parameters. This matches the editor's format
    /// (see godot-vscode-plugin server_controller.ts send_command).
    pub fn send_command(&self, command: &str, args: &[GodotVariant]) -> bool {
        let items = vec![
            GodotVariant::String(command.to_string()),
            GodotVariant::Int(1), // thread_id (main thread)
            GodotVariant::Array(args.to_vec()),
        ];
        let packet = encode_packet(&items);
        eprintln!(
            "debug_server: send {command} ({} bytes) args={args:?}",
            packet.len()
        );

        let mut guard = self.stream.lock().unwrap();
        if let Some(ref mut stream) = *guard {
            match stream.write_all(&packet) {
                Ok(()) => {
                    let _ = stream.flush();
                    true
                }
                Err(e) => {
                    eprintln!("debug_server: write failed: {e}");
                    false
                }
            }
        } else {
            eprintln!("debug_server: no connection");
            false
        }
    }

    /// Wait for a specific response message (by command prefix), with timeout.
    pub fn wait_message(&self, prefix: &str, timeout: Duration) -> Option<Vec<GodotVariant>> {
        self.inbox.wait_for(prefix, timeout)
    }

    /// Wait for any of several response messages, returning the first match.
    pub fn wait_message_any(
        &self,
        prefixes: &[&str],
        timeout: Duration,
    ) -> Option<Vec<GodotVariant>> {
        self.inbox.wait_for_any(prefixes, timeout)
    }

    // ═══════════════════════════════════════════════════════════════════
    // Core debugger commands (remote_debugger.cpp)
    // ═══════════════════════════════════════════════════════════════════

    // ── Execution control ──

    pub fn cmd_continue(&self) -> bool {
        self.send_command("continue", &[])
    }

    pub fn cmd_break(&self) -> bool {
        self.send_command("break", &[])
    }

    pub fn cmd_next(&self) -> bool {
        self.send_command("next", &[])
    }

    pub fn cmd_step(&self) -> bool {
        self.send_command("step", &[])
    }

    pub fn cmd_out(&self) -> bool {
        self.send_command("out", &[])
    }

    // ── Breakpoints ──

    pub fn cmd_breakpoint(&self, path: &str, line: u32, enabled: bool) -> bool {
        self.send_command(
            "breakpoint",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(line)),
                GodotVariant::Bool(enabled),
            ],
        )
    }

    pub fn cmd_set_skip_breakpoints(&self, skip: bool) -> bool {
        self.send_command("set_skip_breakpoints", &[GodotVariant::Bool(skip)])
    }

    pub fn cmd_set_ignore_error_breaks(&self, ignore: bool) -> bool {
        self.send_command("set_ignore_error_breaks", &[GodotVariant::Bool(ignore)])
    }

    // ── Stack inspection ──

    pub fn cmd_get_stack_dump(&self) -> Option<Vec<StackFrameInfo>> {
        if !self.send_command("get_stack_dump", &[]) {
            return None;
        }
        let msg = self.wait_message("stack_dump", Duration::from_secs(5))?;
        Some(parse_stack_dump(&msg))
    }

    pub fn cmd_get_stack_frame_vars(&self, frame: u32) -> Option<FrameVariables> {
        if !self.send_command(
            "get_stack_frame_vars",
            &[GodotVariant::Int(i64::from(frame))],
        ) {
            return None;
        }
        let counts_msg = self.wait_message("stack_frame_vars", Duration::from_secs(5))?;
        let (local_count, member_count, global_count) = parse_var_counts(&counts_msg)?;
        let total = local_count + member_count + global_count;

        let mut vars = Vec::with_capacity(total);
        for _ in 0..total {
            if let Some(var_msg) = self.wait_message("stack_frame_var", Duration::from_secs(2))
                && let Some(v) = parse_debug_variable(&var_msg)
            {
                vars.push(v);
            }
        }

        let mut locals = Vec::new();
        let mut members = Vec::new();
        let mut globals = Vec::new();
        for (i, v) in vars.into_iter().enumerate() {
            if i < local_count {
                locals.push(v);
            } else if i < local_count + member_count {
                members.push(v);
            } else {
                globals.push(v);
            }
        }

        Some(FrameVariables {
            locals,
            members,
            globals,
        })
    }

    // ── Expression evaluation ──

    pub fn cmd_evaluate(&self, expr: &str, frame: u32) -> Option<EvalResult> {
        if !self.send_command(
            "evaluate",
            &[
                GodotVariant::String(expr.to_string()),
                GodotVariant::Int(i64::from(frame)),
            ],
        ) {
            return None;
        }
        let msg = self.wait_message("evaluation_return", Duration::from_secs(5))?;
        parse_eval_result(&msg)
    }

    // ── Script reloading ──

    pub fn cmd_reload_scripts(&self) -> bool {
        self.send_command("reload_scripts", &[])
    }

    pub fn cmd_reload_all_scripts(&self) -> bool {
        self.send_command("reload_all_scripts", &[])
    }

    // ═══════════════════════════════════════════════════════════════════
    // Scene debugger commands (scene_debugger.cpp, prefix "scene:")
    // ═══════════════════════════════════════════════════════════════════

    // ── Scene tree ──

    pub fn cmd_request_scene_tree(&self) -> Option<SceneTree> {
        if !self.send_command("scene:request_scene_tree", &[]) {
            return None;
        }
        let msg = self.wait_message("scene:scene_tree", Duration::from_secs(5))?;
        Some(parse_scene_tree(&msg))
    }

    // ── Object inspection ──

    pub fn cmd_inspect_object(&self, object_id: u64) -> Option<ObjectInfo> {
        // Godot 4.2+: use inspect_objects (plural).
        // Format: [Array([id1, ...]), Bool(update_selection)]
        // Success response: "scene:inspect_objects" with serialized object data
        // Missing object: "remote_selection_invalidated" + "remote_nothing_selected"
        let ids_array = GodotVariant::Array(vec![GodotVariant::Int(object_id as i64)]);
        if !self.send_command(
            "scene:inspect_objects",
            &[ids_array, GodotVariant::Bool(false)],
        ) {
            return None;
        }
        // Drain stale responses until we get the one matching our object_id.
        // The inbox may contain leftover inspect_objects responses from prior requests.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let msg = self.wait_message_any(
                &[
                    "scene:inspect_objects",
                    "remote_selection_invalidated",
                    "remote_nothing_selected",
                ],
                remaining,
            )?;

            match msg.first() {
                Some(GodotVariant::String(cmd)) if cmd == "scene:inspect_objects" => {
                    if let Some(info) = parse_object_info(&msg) {
                        if info.object_id == object_id {
                            return Some(info);
                        }
                        // Stale response for a different object — discard and keep waiting
                        continue;
                    }
                    return None;
                }
                Some(GodotVariant::String(cmd))
                    if cmd == "remote_selection_invalidated"
                        || cmd == "remote_nothing_selected" =>
                {
                    // Object not found in Godot's ObjectDB. Drain the paired message
                    // (invalidated comes with nothing_selected, or vice versa).
                    let _ = self.wait_message_any(
                        &["remote_nothing_selected", "remote_selection_invalidated"],
                        Duration::from_millis(500),
                    );
                    eprintln!("debug_server: object {object_id} not found in Godot's ObjectDB");
                    return None;
                }
                _ => return None,
            }
        }
    }

    /// Inspect multiple objects at once (Godot 4.x).
    /// Format: [Array([id1, id2, ...]), Bool(selection)]
    /// Returns info for the first object in the response.
    pub fn cmd_inspect_objects(&self, ids: &[u64], selection: bool) -> Option<Vec<ObjectInfo>> {
        let ids_array =
            GodotVariant::Array(ids.iter().map(|&id| GodotVariant::Int(id as i64)).collect());
        if !self.send_command(
            "scene:inspect_objects",
            &[ids_array, GodotVariant::Bool(selection)],
        ) {
            return None;
        }
        // Collect responses for each requested object
        let mut results = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in ids {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let msg = self.wait_message_any(
                &[
                    "scene:inspect_objects",
                    "remote_selection_invalidated",
                    "remote_nothing_selected",
                ],
                remaining,
            );
            match msg.as_deref().and_then(|m| m.first()) {
                Some(GodotVariant::String(cmd)) if cmd == "scene:inspect_objects" => {
                    if let Some(info) = parse_object_info(msg.as_deref().unwrap()) {
                        results.push(info);
                    }
                }
                _ => break,
            }
        }
        Some(results)
    }

    pub fn cmd_clear_selection(&self) -> bool {
        self.send_command("scene:clear_selection", &[])
    }

    /// Save a node to a file on the game's filesystem.
    /// Save a node to a file. Returns the saved file path from Godot's confirmation.
    pub fn cmd_save_node(&self, object_id: u64, path: &str) -> Option<String> {
        if !self.send_command(
            "scene:save_node",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(path.to_string()),
            ],
        ) {
            return None;
        }
        // Godot responds with "filesystem:update_file" [path]
        let msg = self.wait_message_any(&["filesystem:update_file"], Duration::from_secs(5));
        match msg {
            Some(ref m) if m.len() >= 2 => variant_as_string(&m[1]),
            _ => Some(path.to_string()), // Command sent, assume success
        }
    }

    // ── Property modification ──

    pub fn cmd_set_object_property(
        &self,
        object_id: u64,
        property: &str,
        value: GodotVariant,
    ) -> bool {
        self.send_command(
            "scene:set_object_property",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(property.to_string()),
                value,
            ],
        )
    }

    /// Set a specific field within a property (e.g. Vector3.x).
    pub fn cmd_set_object_property_field(
        &self,
        object_id: u64,
        property: &str,
        value: GodotVariant,
        field: &str,
    ) -> bool {
        self.send_command(
            "scene:set_object_property_field",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(property.to_string()),
                value,
                GodotVariant::String(field.to_string()),
            ],
        )
    }

    // ── Game control ──

    pub fn cmd_suspend(&self, suspend: bool) -> bool {
        self.send_command("scene:suspend_changed", &[GodotVariant::Bool(suspend)])
    }

    pub fn cmd_next_frame(&self) -> bool {
        self.send_command("scene:next_frame", &[])
    }

    pub fn cmd_set_speed(&self, scale: f64) -> bool {
        self.send_command("scene:speed_changed", &[GodotVariant::Float(scale)])
    }

    pub fn cmd_mute_audio(&self, mute: bool) -> bool {
        self.send_command("scene:debug_mute_audio", &[GodotVariant::Bool(mute)])
    }

    /// Reload specific cached files in the running game.
    pub fn cmd_reload_cached_files(&self, paths: &[&str]) -> bool {
        let args: Vec<GodotVariant> = paths
            .iter()
            .map(|p| GodotVariant::String(p.to_string()))
            .collect();
        self.send_command("scene:reload_cached_files", &args)
    }

    // ── Camera override ──

    /// Enable/disable camera override (take control of the game camera).
    pub fn cmd_override_cameras(&self, enable: bool) -> bool {
        self.send_command(
            "scene:override_cameras",
            &[GodotVariant::Bool(enable), GodotVariant::Bool(true)],
        )
    }

    /// Set the 2D camera transform. `transform` is [xx, xy, yx, yy, ox, oy] (Transform2D).
    pub fn cmd_transform_camera_2d(&self, transform: [f64; 6]) -> bool {
        self.send_command(
            "scene:transform_camera_2d",
            &[GodotVariant::Transform2D(transform)],
        )
    }

    /// Set the 3D camera transform + projection.
    /// `transform` is a 12-element array [basis(9) + origin(3)] (Transform3D).
    pub fn cmd_transform_camera_3d(
        &self,
        transform: [f64; 12],
        perspective: bool,
        fov_or_size: f64,
        near: f64,
        far: f64,
    ) -> bool {
        self.send_command(
            "scene:transform_camera_3d",
            &[
                GodotVariant::Transform3D(transform),
                GodotVariant::Bool(perspective),
                GodotVariant::Float(fov_or_size),
                GodotVariant::Float(near),
                GodotVariant::Float(far),
            ],
        )
    }

    // ── Screenshots ──

    pub fn cmd_request_screenshot(&self, id: u64) -> Option<ScreenshotResult> {
        if !self.send_command("scene:rq_screenshot", &[GodotVariant::Int(id as i64)]) {
            return None;
        }
        // Godot responds with "game_view:get_screenshot" [id, width, height, path]
        let msg = self.wait_message_any(&["game_view:get_screenshot"], Duration::from_secs(10))?;
        // Parse: ["game_view:get_screenshot", Int(id), Int(width), Int(height), String(path)]
        let args = match msg.first() {
            Some(GodotVariant::String(s)) if s == "game_view:get_screenshot" => &msg[1..],
            _ => return None,
        };
        if args.len() < 4 {
            return None;
        }
        Some(ScreenshotResult {
            id: variant_as_u64(&args[0]).unwrap_or(id),
            width: variant_as_u32(&args[1]).unwrap_or(0),
            height: variant_as_u32(&args[2]).unwrap_or(0),
            path: variant_as_string(&args[3]).unwrap_or_default(),
        })
    }

    // ── Runtime node selection ──

    pub fn cmd_runtime_node_select_set_type(&self, node_type: i32) -> bool {
        self.send_command(
            "scene:runtime_node_select_set_type",
            &[GodotVariant::Int(i64::from(node_type))],
        )
    }

    pub fn cmd_runtime_node_select_set_mode(&self, mode: i32) -> bool {
        self.send_command(
            "scene:runtime_node_select_set_mode",
            &[GodotVariant::Int(i64::from(mode))],
        )
    }

    pub fn cmd_runtime_node_select_set_visible(&self, visible: bool) -> bool {
        self.send_command(
            "scene:runtime_node_select_set_visible",
            &[GodotVariant::Bool(visible)],
        )
    }

    pub fn cmd_runtime_node_select_set_avoid_locked(&self, avoid: bool) -> bool {
        self.send_command(
            "scene:runtime_node_select_set_avoid_locked",
            &[GodotVariant::Bool(avoid)],
        )
    }

    pub fn cmd_runtime_node_select_set_prefer_group(&self, prefer: bool) -> bool {
        self.send_command(
            "scene:runtime_node_select_set_prefer_group",
            &[GodotVariant::Bool(prefer)],
        )
    }

    pub fn cmd_runtime_node_select_reset_camera_2d(&self) -> bool {
        self.send_command("scene:runtime_node_select_reset_camera_2d", &[])
    }

    pub fn cmd_runtime_node_select_reset_camera_3d(&self) -> bool {
        self.send_command("scene:runtime_node_select_reset_camera_3d", &[])
    }

    // ═══════════════════════════════════════════════════════════════════
    // Live editing commands (scene_debugger.cpp live editor)
    // ═══════════════════════════════════════════════════════════════════

    /// Set the root scene for live editing.
    pub fn cmd_live_set_root(&self, scene_path: &str, scene_file: &str) -> bool {
        self.send_command(
            "scene:live_set_root",
            &[
                GodotVariant::String(scene_path.to_string()),
                GodotVariant::String(scene_file.to_string()),
            ],
        )
    }

    /// Map a node path to an integer ID for live editing.
    pub fn cmd_live_node_path(&self, path: &str, id: i32) -> bool {
        self.send_command(
            "scene:live_node_path",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(id)),
            ],
        )
    }

    /// Map a resource path to an integer ID for live editing.
    pub fn cmd_live_res_path(&self, path: &str, id: i32) -> bool {
        self.send_command(
            "scene:live_res_path",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(id)),
            ],
        )
    }

    /// Set a property on a live-edited node by ID.
    pub fn cmd_live_node_prop(&self, id: i32, property: &str, value: GodotVariant) -> bool {
        self.send_command(
            "scene:live_node_prop",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                value,
            ],
        )
    }

    /// Set a property on a live-edited node to a resource path.
    pub fn cmd_live_node_prop_res(&self, id: i32, property: &str, res_path: &str) -> bool {
        self.send_command(
            "scene:live_node_prop_res",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                GodotVariant::String(res_path.to_string()),
            ],
        )
    }

    /// Set a property on a live-edited resource by ID.
    pub fn cmd_live_res_prop(&self, id: i32, property: &str, value: GodotVariant) -> bool {
        self.send_command(
            "scene:live_res_prop",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                value,
            ],
        )
    }

    /// Set a property on a live-edited resource to another resource path.
    pub fn cmd_live_res_prop_res(&self, id: i32, property: &str, res_path: &str) -> bool {
        self.send_command(
            "scene:live_res_prop_res",
            &[
                GodotVariant::Int(i64::from(id)),
                GodotVariant::String(property.to_string()),
                GodotVariant::String(res_path.to_string()),
            ],
        )
    }

    /// Call a method on a live-edited node.
    pub fn cmd_live_node_call(&self, id: i32, method: &str, args: &[GodotVariant]) -> bool {
        let mut cmd_args = vec![
            GodotVariant::Int(i64::from(id)),
            GodotVariant::String(method.to_string()),
        ];
        cmd_args.extend_from_slice(args);
        self.send_command("scene:live_node_call", &cmd_args)
    }

    /// Call a method on a live-edited resource.
    pub fn cmd_live_res_call(&self, id: i32, method: &str, args: &[GodotVariant]) -> bool {
        let mut cmd_args = vec![
            GodotVariant::Int(i64::from(id)),
            GodotVariant::String(method.to_string()),
        ];
        cmd_args.extend_from_slice(args);
        self.send_command("scene:live_res_call", &cmd_args)
    }

    /// Create a new node in the live scene.
    pub fn cmd_live_create_node(&self, parent_path: &str, class: &str, name: &str) -> bool {
        self.send_command(
            "scene:live_create_node",
            &[
                GodotVariant::String(parent_path.to_string()),
                GodotVariant::String(class.to_string()),
                GodotVariant::String(name.to_string()),
            ],
        )
    }

    /// Instantiate a packed scene as a child of a node.
    pub fn cmd_live_instantiate_node(
        &self,
        parent_path: &str,
        scene_path: &str,
        name: &str,
    ) -> bool {
        self.send_command(
            "scene:live_instantiate_node",
            &[
                GodotVariant::String(parent_path.to_string()),
                GodotVariant::String(scene_path.to_string()),
                GodotVariant::String(name.to_string()),
            ],
        )
    }

    /// Remove a node from the live scene.
    pub fn cmd_live_remove_node(&self, path: &str) -> bool {
        self.send_command(
            "scene:live_remove_node",
            &[GodotVariant::String(path.to_string())],
        )
    }

    /// Remove a node but keep it (for later restore).
    pub fn cmd_live_remove_and_keep_node(&self, path: &str, object_id: u64) -> bool {
        self.send_command(
            "scene:live_remove_and_keep_node",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(object_id as i64),
            ],
        )
    }

    /// Restore a previously removed-and-kept node.
    pub fn cmd_live_restore_node(&self, object_id: u64, path: &str, pos: i32) -> bool {
        self.send_command(
            "scene:live_restore_node",
            &[
                GodotVariant::Int(object_id as i64),
                GodotVariant::String(path.to_string()),
                GodotVariant::Int(i64::from(pos)),
            ],
        )
    }

    /// Duplicate a node in the live scene.
    pub fn cmd_live_duplicate_node(&self, path: &str, new_name: &str) -> bool {
        self.send_command(
            "scene:live_duplicate_node",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::String(new_name.to_string()),
            ],
        )
    }

    /// Reparent a node in the live scene.
    pub fn cmd_live_reparent_node(
        &self,
        path: &str,
        new_parent: &str,
        new_name: &str,
        pos: i32,
    ) -> bool {
        self.send_command(
            "scene:live_reparent_node",
            &[
                GodotVariant::String(path.to_string()),
                GodotVariant::String(new_parent.to_string()),
                GodotVariant::String(new_name.to_string()),
                GodotVariant::Int(i64::from(pos)),
            ],
        )
    }

    // ═══════════════════════════════════════════════════════════════════
    // Profiler commands
    // ═══════════════════════════════════════════════════════════════════

    /// Toggle a profiler (scripts, visual, servers).
    pub fn cmd_toggle_profiler(&self, profiler_name: &str, enable: bool) -> bool {
        let cmd = format!("profiler:{profiler_name}");
        self.send_command(&cmd, &[GodotVariant::Bool(enable)])
    }

    // ── Internal ──

    fn spawn_reader(&self, stream: TcpStream) {
        let inbox = Arc::clone(&self.inbox);
        let running = Arc::clone(&self.running);

        std::thread::spawn(move || {
            reader_loop(stream, &inbox, &running);
        });
    }
}

impl Drop for GodotDebugServer {
    fn drop(&mut self) {
        *self.running.lock().unwrap() = false;
    }
}

// ---------------------------------------------------------------------------
// Reader thread
// ---------------------------------------------------------------------------

fn reader_loop(mut stream: TcpStream, inbox: &Inbox, running: &Mutex<bool>) {
    // Set a short read timeout so we can check the `running` flag periodically
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));

    loop {
        if !*running.lock().unwrap() {
            break;
        }

        // Read 4-byte length header
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => break, // Disconnected or other error
        }
        let payload_len = u32::from_le_bytes(len_buf) as usize;

        // Sanity check: reject absurdly large messages (> 64MB)
        if payload_len > 64 * 1024 * 1024 {
            break;
        }

        // Read payload
        let mut payload = vec![0u8; payload_len];
        if stream.read_exact(&mut payload).is_err() {
            break;
        }

        // Build full packet (length + payload) for decode_packet
        let mut full = Vec::with_capacity(4 + payload_len);
        full.extend_from_slice(&len_buf);
        full.extend_from_slice(&payload);

        if let Some(items) = decode_packet(&full) {
            // Normalize from wire format [cmd, thread_id, Array(data)]
            // to flat [cmd, data_items...] for downstream parsing
            let items = normalize_message(items);
            inbox.push(items);
        } else {
            // Diagnostic: try to find where decoding fails
            let mut diag_offset = 4; // skip length header
            if let Some(header) = crate::debug::variant::decode_variant(&full, &mut diag_offset) {
                eprintln!(
                    "debug_server: packet decoded as {header} but expected Array ({payload_len} bytes)"
                );
            } else {
                // Show first bytes for diagnosis
                let preview: Vec<String> =
                    full.iter().take(64).map(|b| format!("{b:02x}")).collect();
                eprintln!(
                    "debug_server: failed to decode packet ({payload_len} bytes), first bytes: {}",
                    preview.join(" ")
                );
            }
        }
    }
}

/// Normalize a Godot 4.2+ message from wire format [String(cmd), Int(thread_id), Array(data)]
/// into a flat list [String(cmd), data_items...] for downstream parsing.
fn normalize_message(items: Vec<GodotVariant>) -> Vec<GodotVariant> {
    // Expected format: [String(cmd), Int(thread_id), Array([args...])]
    if items.len() >= 3
        && let Some(GodotVariant::String(_)) = items.first()
        && let Some(GodotVariant::Int(_)) = items.get(1)
        && let Some(GodotVariant::Array(_)) = items.get(2)
    {
        let mut result = Vec::new();
        result.push(items[0].clone()); // command name
        // Flatten the inner data array
        if let GodotVariant::Array(data) = &items[2] {
            result.extend_from_slice(data);
        }
        return result;
    }
    // Fallback: return as-is (older protocol or unknown format)
    items
}

// ---------------------------------------------------------------------------
// Response parsers
// ---------------------------------------------------------------------------

/// Parse stack_dump response: [String("stack_dump"), String(file), Int(line), String(func), ...]
fn parse_stack_dump(msg: &[GodotVariant]) -> Vec<StackFrameInfo> {
    let mut frames = Vec::new();
    // Skip the command name at index 0
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "stack_dump"))
    {
        &msg[1..]
    } else {
        msg
    };

    // Triplets: [String(file), Int(line), String(function)]
    for chunk in args.chunks(3) {
        if chunk.len() < 3 {
            break;
        }
        if let (GodotVariant::String(file), GodotVariant::Int(line), GodotVariant::String(func)) =
            (&chunk[0], &chunk[1], &chunk[2])
        {
            frames.push(StackFrameInfo {
                file: file.clone(),
                line: *line as u32,
                function: func.clone(),
            });
        }
    }
    frames
}

/// Parse var counts: [String("stack_frame_vars"), Int(local), Int(member), Int(global)]
fn parse_var_counts(msg: &[GodotVariant]) -> Option<(usize, usize, usize)> {
    // Skip command name
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "stack_frame_vars"))
    {
        &msg[1..]
    } else {
        msg
    };
    if args.len() < 3 {
        return None;
    }
    let local = variant_as_usize(&args[0])?;
    let member = variant_as_usize(&args[1])?;
    let global = variant_as_usize(&args[2])?;
    Some((local, member, global))
}

/// Parse a single variable message: [String("stack_frame_var"), String(name), Int(type), Variant(value)]
fn parse_debug_variable(msg: &[GodotVariant]) -> Option<DebugVariable> {
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "stack_frame_var"))
    {
        &msg[1..]
    } else {
        msg
    };
    if args.len() < 3 {
        return None;
    }
    let name = variant_as_string(&args[0])?;
    let var_type = variant_as_i32(&args[1])?;
    let value = args.get(2).cloned().unwrap_or(GodotVariant::Nil);
    Some(DebugVariable {
        name,
        value,
        var_type,
    })
}

/// Parse evaluation_return: [String("evaluation_return"), String(name), Int(type), Variant(value)]
fn parse_eval_result(msg: &[GodotVariant]) -> Option<EvalResult> {
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "evaluation_return"))
    {
        &msg[1..]
    } else {
        msg
    };
    if args.len() < 3 {
        return None;
    }
    let name = variant_as_string(&args[0])?;
    let var_type = variant_as_i32(&args[1])?;
    let value = args.get(2).cloned().unwrap_or(GodotVariant::Nil);
    Some(EvalResult {
        name,
        value,
        var_type,
    })
}

/// Parse scene:scene_tree response.
/// Wire format (after normalization): [String("scene:scene_tree"), ...node data...]
/// Each node is 6 sequential fields followed by its children (recursive):
///   Int(child_count), String(name), String(class), Int(object_id),
///   String(scene_file_path), Int(view_flags)
/// The root node is a single node (not a list), matching the VS Code plugin's
/// `parse_next_scene_node` in helpers.ts.
fn parse_scene_tree(msg: &[GodotVariant]) -> SceneTree {
    let args = if msg
        .first()
        .is_some_and(|v| matches!(v, GodotVariant::String(s) if s == "scene:scene_tree"))
    {
        &msg[1..]
    } else {
        msg
    };
    let mut offset = 0;
    if let Some(root) = parse_scene_node(args, &mut offset) {
        SceneTree { nodes: vec![root] }
    } else {
        SceneTree { nodes: Vec::new() }
    }
}

/// Parse a single scene node and its children recursively.
/// Each node: [child_count, name, class_name, object_id, scene_file_path, view_flags]
fn parse_scene_node(args: &[GodotVariant], offset: &mut usize) -> Option<SceneNode> {
    if *offset >= args.len() {
        return None;
    }

    let child_count = variant_as_usize(&args[*offset])?;
    *offset += 1;

    let name = variant_as_string(args.get(*offset)?).unwrap_or_default();
    *offset += 1;
    let class_name = variant_as_string(args.get(*offset)?).unwrap_or_default();
    *offset += 1;
    let object_id = variant_as_u64(args.get(*offset)?).unwrap_or(0);
    *offset += 1;
    let scene_file_path = variant_as_string(args.get(*offset)?);
    *offset += 1;
    // view_flags — skip
    *offset += 1;

    let mut children = Vec::new();
    for _ in 0..child_count {
        if let Some(child) = parse_scene_node(args, offset) {
            children.push(child);
        } else {
            break;
        }
    }

    let scene_file_path = scene_file_path.filter(|s| !s.is_empty());
    Some(SceneNode {
        name,
        class_name,
        object_id,
        scene_file_path,
        children,
    })
}

/// Parse object info from an inspect response.
///
/// Godot 4.6 format (after normalization):
/// `[String(cmd), Array([Int(id), String(class), Array([Array([prop_fields...]), ...])])]`
///
/// Each property is an Array of 6 elements:
/// `[String(name), Int(type), Int(hint), String(hint_string), Int(usage), Variant(value)]`
fn parse_object_info(msg: &[GodotVariant]) -> Option<ObjectInfo> {
    // Strip command prefix
    let after_cmd = match msg.first() {
        Some(GodotVariant::String(s))
            if s == "scene:inspect_object" || s == "scene:inspect_objects" =>
        {
            &msg[1..]
        }
        _ => msg,
    };

    // Unwrap outer Array: [Array([id, class, Array([props...])])]
    let args: &[GodotVariant];
    let owned;
    if let Some(GodotVariant::Array(inner)) = after_cmd.first() {
        owned = inner.clone();
        args = &owned;
    } else {
        args = after_cmd;
    };

    if args.len() < 2 {
        return None;
    }
    let object_id = variant_as_u64(&args[0])?;
    let class_name = variant_as_string(&args[1])?;

    let mut properties = Vec::new();

    // Godot 4.6: args[2] is Array([Array([prop1...]), Array([prop2...]), ...])
    if let Some(GodotVariant::Array(prop_arrays)) = args.get(2) {
        for prop_arr in prop_arrays {
            if let GodotVariant::Array(fields) = prop_arr
                && fields.len() >= 6
            {
                let raw_name = variant_as_string(&fields[0]).unwrap_or_default();
                let name = strip_property_prefix(&raw_name);
                let type_id = variant_as_u32(&fields[1]).unwrap_or(0);
                let hint = variant_as_u32(&fields[2]).unwrap_or(0);
                let hint_string = variant_as_string(&fields[3]).unwrap_or_default();
                let usage = variant_as_u32(&fields[4]).unwrap_or(0);
                let value = fields[5].clone();
                properties.push(ObjectProperty {
                    name,
                    value,
                    type_id,
                    hint,
                    hint_string,
                    usage,
                });
            }
        }
    } else {
        // Fallback: flat format [name, type, hint, hint_string, usage, value, ...]
        let prop_data = &args[2..];
        for chunk in prop_data.chunks(6) {
            if chunk.len() < 6 {
                break;
            }
            let raw_name = variant_as_string(&chunk[0]).unwrap_or_default();
            let name = strip_property_prefix(&raw_name);
            let type_id = variant_as_u32(&chunk[1]).unwrap_or(0);
            let hint = variant_as_u32(&chunk[2]).unwrap_or(0);
            let hint_string = variant_as_string(&chunk[3]).unwrap_or_default();
            let usage = variant_as_u32(&chunk[4]).unwrap_or(0);
            let value = chunk[5].clone();
            properties.push(ObjectProperty {
                name,
                value,
                type_id,
                hint,
                hint_string,
                usage,
            });
        }
    }

    Some(ObjectInfo {
        object_id,
        class_name,
        properties,
    })
}

/// Strip Godot section prefixes from property names.
/// Godot's binary protocol sends names like "Members/position", "Constants/STATE_IDLE".
/// These prefixes are for categorization only — `set-prop` doesn't use them.
fn strip_property_prefix(name: &str) -> String {
    for prefix in &["Members/", "Constants/"] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return stripped.to_string();
        }
    }
    name.to_string()
}

// ---------------------------------------------------------------------------
// Variant helpers
// ---------------------------------------------------------------------------

fn variant_as_string(v: &GodotVariant) -> Option<String> {
    match v {
        GodotVariant::String(s) | GodotVariant::StringName(s) => Some(s.clone()),
        _ => None,
    }
}

fn variant_as_i32(v: &GodotVariant) -> Option<i32> {
    match v {
        GodotVariant::Int(i) => Some(*i as i32),
        _ => None,
    }
}

fn variant_as_u32(v: &GodotVariant) -> Option<u32> {
    match v {
        GodotVariant::Int(i) => Some(*i as u32),
        _ => None,
    }
}

fn variant_as_u64(v: &GodotVariant) -> Option<u64> {
    match v {
        GodotVariant::Int(i) => Some(*i as u64),
        GodotVariant::ObjectId(id) => Some(*id),
        _ => None,
    }
}

fn variant_as_usize(v: &GodotVariant) -> Option<usize> {
    match v {
        GodotVariant::Int(i) => Some(*i as usize),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_command_encoding() {
        // Verify that send_command produces the right packet structure
        let items = vec![
            GodotVariant::String("breakpoint".to_string()),
            GodotVariant::String("res://main.gd".to_string()),
            GodotVariant::Int(10),
            GodotVariant::Bool(true),
        ];
        let packet = encode_packet(&items);
        let decoded = decode_packet(&packet).unwrap();
        assert_eq!(decoded.len(), 4);
        assert_eq!(decoded[0], GodotVariant::String("breakpoint".to_string()));
        assert_eq!(
            decoded[1],
            GodotVariant::String("res://main.gd".to_string())
        );
        assert_eq!(decoded[2], GodotVariant::Int(10));
        assert_eq!(decoded[3], GodotVariant::Bool(true));
    }

    #[test]
    fn test_parse_stack_dump() {
        let msg = vec![
            GodotVariant::String("stack_dump".into()),
            GodotVariant::String("res://main.gd".into()),
            GodotVariant::Int(15),
            GodotVariant::String("_ready".into()),
            GodotVariant::String("res://player.gd".into()),
            GodotVariant::Int(42),
            GodotVariant::String("move".into()),
        ];
        let frames = parse_stack_dump(&msg);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].file, "res://main.gd");
        assert_eq!(frames[0].line, 15);
        assert_eq!(frames[0].function, "_ready");
        assert_eq!(frames[1].file, "res://player.gd");
        assert_eq!(frames[1].line, 42);
        assert_eq!(frames[1].function, "move");
    }

    #[test]
    fn test_parse_stack_dump_empty() {
        let msg = vec![GodotVariant::String("stack_dump".into())];
        let frames = parse_stack_dump(&msg);
        assert!(frames.is_empty());
    }

    #[test]
    fn test_parse_var_counts() {
        let msg = vec![
            GodotVariant::String("stack_frame_vars".into()),
            GodotVariant::Int(3),
            GodotVariant::Int(2),
            GodotVariant::Int(1),
        ];
        let (l, m, g) = parse_var_counts(&msg).unwrap();
        assert_eq!(l, 3);
        assert_eq!(m, 2);
        assert_eq!(g, 1);
    }

    #[test]
    fn test_parse_var_counts_missing_returns_none() {
        let msg = vec![
            GodotVariant::String("stack_frame_vars".into()),
            GodotVariant::Int(3),
        ];
        assert!(parse_var_counts(&msg).is_none());
    }

    #[test]
    fn test_parse_debug_variable() {
        let msg = vec![
            GodotVariant::String("stack_frame_var".into()),
            GodotVariant::String("health".into()),
            GodotVariant::Int(2), // type
            GodotVariant::Int(100),
        ];
        let v = parse_debug_variable(&msg).unwrap();
        assert_eq!(v.name, "health");
        assert_eq!(v.var_type, 2);
        assert_eq!(v.value, GodotVariant::Int(100));
    }

    #[test]
    fn test_parse_eval_result() {
        let msg = vec![
            GodotVariant::String("evaluation_return".into()),
            GodotVariant::String("2 + 2".into()),
            GodotVariant::Int(2),
            GodotVariant::Int(4),
        ];
        let r = parse_eval_result(&msg).unwrap();
        assert_eq!(r.name, "2 + 2");
        assert_eq!(r.var_type, 2);
        assert_eq!(r.value, GodotVariant::Int(4));
    }

    #[test]
    fn test_parse_object_info_nested() {
        // Godot 4.6 format: [cmd, Array([id, class, Array([Array([prop...]), ...])])]
        let msg = vec![
            GodotVariant::String("scene:inspect_objects".into()),
            GodotVariant::Array(vec![
                GodotVariant::Int(1234),
                GodotVariant::String("Node2D".into()),
                GodotVariant::Array(vec![GodotVariant::Array(vec![
                    GodotVariant::String("position".into()),
                    GodotVariant::Int(5), // TYPE_VECTOR2
                    GodotVariant::Int(0),
                    GodotVariant::String("".into()),
                    GodotVariant::Int(6),
                    GodotVariant::Vector2(10.0, 20.0),
                ])]),
            ]),
        ];
        let info = parse_object_info(&msg).unwrap();
        assert_eq!(info.object_id, 1234);
        assert_eq!(info.class_name, "Node2D");
        assert_eq!(info.properties.len(), 1);
        assert_eq!(info.properties[0].name, "position");
        assert_eq!(info.properties[0].type_id, 5);
    }

    #[test]
    fn test_parse_object_info_flat_fallback() {
        // Legacy flat format
        let msg = vec![
            GodotVariant::String("scene:inspect_object".into()),
            GodotVariant::Int(1234),
            GodotVariant::String("Node2D".into()),
            GodotVariant::String("position".into()),
            GodotVariant::Int(5),
            GodotVariant::Int(0),
            GodotVariant::String("".into()),
            GodotVariant::Int(6),
            GodotVariant::Vector2(10.0, 20.0),
        ];
        let info = parse_object_info(&msg).unwrap();
        assert_eq!(info.object_id, 1234);
        assert_eq!(info.class_name, "Node2D");
        assert_eq!(info.properties.len(), 1);
        assert_eq!(info.properties[0].name, "position");
    }

    #[test]
    fn test_normalize_message() {
        // Godot 4.2+ wire format: [cmd, thread_id, Array(data)]
        let msg = vec![
            GodotVariant::String("debug_enter".into()),
            GodotVariant::Int(1), // thread_id
            GodotVariant::Array(vec![
                GodotVariant::Bool(true),
                GodotVariant::String("Breakpoint".into()),
                GodotVariant::Bool(true),
            ]),
        ];
        let normalized = normalize_message(msg);
        assert_eq!(normalized.len(), 4);
        assert_eq!(normalized[0], GodotVariant::String("debug_enter".into()));
        assert_eq!(normalized[1], GodotVariant::Bool(true));
        assert_eq!(normalized[2], GodotVariant::String("Breakpoint".into()));
    }

    #[test]
    fn test_normalize_message_empty_data() {
        // scene:scene_tree with actual tree data in the inner array
        let msg = vec![
            GodotVariant::String("scene:scene_tree".into()),
            GodotVariant::Int(1),
            GodotVariant::Array(vec![GodotVariant::Int(2)]), // child count
        ];
        let normalized = normalize_message(msg);
        assert_eq!(normalized.len(), 2);
        assert_eq!(
            normalized[0],
            GodotVariant::String("scene:scene_tree".into())
        );
        assert_eq!(normalized[1], GodotVariant::Int(2));
    }

    #[test]
    fn test_msg_matches() {
        let msg = vec![
            GodotVariant::String("stack_dump".into()),
            GodotVariant::Int(1),
        ];
        assert!(msg_matches(&msg, "stack_dump"));
        assert!(!msg_matches(&msg, "debug_enter"));
    }

    #[test]
    fn test_server_new_and_port() {
        let server = GodotDebugServer::new(0).unwrap();
        assert!(server.port() > 0);
        assert!(!server.is_connected());
    }

    #[test]
    fn test_accept_timeout() {
        let server = GodotDebugServer::new(0).unwrap();
        // Accept with very short timeout should return false (no one connecting)
        assert!(!server.accept(Duration::from_millis(10)));
    }

    #[test]
    fn test_send_without_connection() {
        let server = GodotDebugServer::new(0).unwrap();
        assert!(!server.send_command("continue", &[]));
    }

    #[test]
    fn test_connection_and_send() {
        let server = GodotDebugServer::new(0).unwrap();
        let port = server.port();

        // Simulate a game connecting
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            TcpStream::connect(format!("127.0.0.1:{port}")).unwrap()
        });

        assert!(server.accept(Duration::from_secs(2)));
        assert!(server.is_connected());
        assert!(server.send_command("continue", &[]));

        let _client = handle.join().unwrap();
    }

    #[test]
    fn test_inbox_push_and_wait() {
        let inbox = Inbox::new();
        inbox.push(vec![
            GodotVariant::String("stack_dump".into()),
            GodotVariant::String("res://main.gd".into()),
            GodotVariant::Int(10),
            GodotVariant::String("_ready".into()),
        ]);

        let msg = inbox
            .wait_for("stack_dump", Duration::from_secs(1))
            .unwrap();
        assert_eq!(msg.len(), 4);
    }

    #[test]
    fn test_inbox_wait_timeout() {
        let inbox = Inbox::new();
        let result = inbox.wait_for("stack_dump", Duration::from_millis(10));
        assert!(result.is_none());
    }

    #[test]
    fn test_inbox_wait_concurrent() {
        let inbox = Arc::new(Inbox::new());
        let inbox2 = Arc::clone(&inbox);

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            inbox2.push(vec![GodotVariant::String("debug_enter".into())]);
        });

        let msg = inbox
            .wait_for("debug_enter", Duration::from_secs(2))
            .unwrap();
        assert_eq!(msg.len(), 1);
    }

    #[test]
    fn test_inbox_wait_for_any() {
        let inbox = Inbox::new();
        // Push a "remote_nothing_selected" message
        inbox.push(vec![
            GodotVariant::String("remote_nothing_selected".into()),
            GodotVariant::Int(0),
        ]);
        // wait_for_any should match it
        let msg = inbox
            .wait_for_any(
                &[
                    "scene:inspect_objects",
                    "remote_nothing_selected",
                    "remote_selection_invalidated",
                ],
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(
            msg[0],
            GodotVariant::String("remote_nothing_selected".into())
        );
    }

    #[test]
    fn test_inbox_wait_for_any_prefers_first_match() {
        let inbox = Inbox::new();
        // Push two messages
        inbox.push(vec![GodotVariant::String(
            "remote_selection_invalidated".into(),
        )]);
        inbox.push(vec![GodotVariant::String("scene:inspect_objects".into())]);
        // Should return the first one found (insertion order)
        let msg = inbox
            .wait_for_any(
                &["scene:inspect_objects", "remote_selection_invalidated"],
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(
            msg[0],
            GodotVariant::String("remote_selection_invalidated".into())
        );
    }

    #[test]
    fn test_parse_scene_tree() {
        // Root node with 1 child, that child has 0 children
        // Format per node: [child_count, name, class, id, scene_file, view_flags]
        let msg = vec![
            GodotVariant::String("scene:scene_tree".into()),
            // Root node
            GodotVariant::Int(1),                  // child_count
            GodotVariant::String("root".into()),   // name
            GodotVariant::String("Window".into()), // class
            GodotVariant::Int(1234),               // object_id
            GodotVariant::String("".into()),       // scene_file_path
            GodotVariant::Int(0),                  // view_flags
            // Child node
            GodotVariant::Int(0),                             // child_count
            GodotVariant::String("Player".into()),            // name
            GodotVariant::String("CharacterBody3D".into()),   // class
            GodotVariant::Int(5678),                          // object_id
            GodotVariant::String("res://player.tscn".into()), // scene_file_path
            GodotVariant::Int(0),                             // view_flags
        ];
        let tree = parse_scene_tree(&msg);
        assert_eq!(tree.nodes.len(), 1); // single root
        let root = &tree.nodes[0];
        assert_eq!(root.name, "root");
        assert_eq!(root.class_name, "Window");
        assert_eq!(root.object_id, 1234);
        assert_eq!(root.children.len(), 1);
        let child = &root.children[0];
        assert_eq!(child.name, "Player");
        assert_eq!(child.class_name, "CharacterBody3D");
        assert_eq!(child.scene_file_path.as_deref(), Some("res://player.tscn"));
        assert!(child.children.is_empty());
    }

    #[test]
    fn test_variant_helpers() {
        assert_eq!(
            variant_as_string(&GodotVariant::String("hello".into())),
            Some("hello".to_string())
        );
        assert_eq!(variant_as_i32(&GodotVariant::Int(42)), Some(42));
        assert_eq!(variant_as_u32(&GodotVariant::Int(42)), Some(42));
        assert_eq!(variant_as_u64(&GodotVariant::Int(42)), Some(42));
        assert_eq!(variant_as_u64(&GodotVariant::ObjectId(99)), Some(99));
        assert_eq!(variant_as_usize(&GodotVariant::Int(5)), Some(5));
        assert!(variant_as_string(&GodotVariant::Int(1)).is_none());
        assert!(variant_as_i32(&GodotVariant::Nil).is_none());
    }
}
