mod args;
mod automation;
mod camera;
mod control;
mod input;
mod live;
mod misc;
mod properties;
mod rewrite;
mod scene;
mod selection;

pub use args::*;

use miette::{Result, miette};
use path_slash::PathExt as _;

pub fn exec(args: &DebugArgs) -> Result<()> {
    match args.command {
        DebugCommand::Stop => crate::cli::stop_cmd::exec(),

        // Execution control
        DebugCommand::Continue(ref a) => control::cmd_exec_continue(a),
        DebugCommand::Pause(ref a) => control::cmd_exec_pause(a),
        DebugCommand::Next(ref a) => control::cmd_exec_next(a),
        DebugCommand::StepIn(ref a) => control::cmd_exec_step_in(a),
        DebugCommand::StepOutFn(ref a) => control::cmd_exec_step_out(a),

        // Debugging
        DebugCommand::Breakpoint(ref a) => control::cmd_breakpoint(a),
        DebugCommand::Stack(ref a) => control::cmd_stack(a),
        DebugCommand::Vars(ref a) => control::cmd_vars(a),
        DebugCommand::Eval(ref a) => control::cmd_evaluate(a),

        // Properties
        DebugCommand::SetProp(ref a) => properties::cmd_set_prop(a),
        DebugCommand::SetPropField(ref a) => properties::cmd_set_prop_field(a),

        // Game loop control
        DebugCommand::Suspend(ref a) => properties::cmd_suspend(a),
        DebugCommand::NextFrame(ref a) => properties::cmd_next_frame(a),
        DebugCommand::TimeScale(ref a) => properties::cmd_time_scale(a),
        DebugCommand::ReloadScripts(ref a) => properties::cmd_reload_scripts(a),
        DebugCommand::ReloadAllScripts(ref a) => properties::cmd_reload_all_scripts(a),
        DebugCommand::SkipBreakpoints(ref a) => properties::cmd_skip_breakpoints(a),
        DebugCommand::IgnoreErrors(ref a) => properties::cmd_ignore_errors(a),
        DebugCommand::MuteAudio(ref a) => misc::cmd_mute_audio(a),
        DebugCommand::Profiler(ref a) => misc::cmd_profiler(a),
        DebugCommand::SaveNode(ref a) => misc::cmd_save_node(a),
        DebugCommand::ReloadCached(ref a) => misc::cmd_reload_cached(a),

        // Input automation (eval-based)
        DebugCommand::Click(ref a) => input::cmd_click(a),
        DebugCommand::Press(ref a) => input::cmd_press(a),
        DebugCommand::Key(ref a) => input::cmd_key(a),
        DebugCommand::Type(ref a) => input::cmd_type_text(a),
        DebugCommand::Wait(ref a) => input::cmd_wait(a),
        DebugCommand::Screenshot(ref a) => camera::cmd_screenshot(a),

        // Node automation (eval-based)
        DebugCommand::Find(ref a) => automation::cmd_find(a),
        DebugCommand::GetProp(ref a) => automation::cmd_get_prop(a),
        DebugCommand::Call(ref a) => automation::cmd_call(a),
        DebugCommand::Set(ref a) => automation::cmd_set(a),
        DebugCommand::Await(ref a) => automation::cmd_await(a),
        DebugCommand::MouseMove(ref a) => automation::cmd_move_to(a),
        DebugCommand::MouseDrag(ref a) => automation::cmd_drag(a),
        DebugCommand::MouseHover(ref a) => automation::cmd_hover(a),

        // Subcommand groups
        DebugCommand::Live(ref a) => exec_live(a),
        DebugCommand::Scene(ref a) => exec_scene(a),
        DebugCommand::Camera(ref a) => exec_camera(a),
        DebugCommand::Select(ref a) => exec_select(a),

        DebugCommand::Server(ref a) => misc::cmd_server(a),
    }
}

fn exec_live(args: &LiveArgs) -> Result<()> {
    ensure_binary_debug()?;
    match args.command {
        LiveCommand::SetRoot(ref a) => live::cmd_live_set_root(a),
        LiveCommand::CreateNode(ref a) => live::cmd_live_create_node(a),
        LiveCommand::Instantiate(ref a) => live::cmd_live_instantiate(a),
        LiveCommand::RemoveNode(ref a) => live::cmd_live_remove_node(a),
        LiveCommand::Duplicate(ref a) => live::cmd_live_duplicate(a),
        LiveCommand::Reparent(ref a) => live::cmd_live_reparent(a),
        LiveCommand::NodeProp(ref a) => live::cmd_live_node_prop(a),
        LiveCommand::NodeCall(ref a) => live::cmd_live_node_call(a),
        LiveCommand::NodePath(ref a) => live::cmd_live_node_path(a),
        LiveCommand::ResPath(ref a) => live::cmd_live_res_path(a),
        LiveCommand::ResProp(ref a) => live::cmd_live_res_prop(a),
        LiveCommand::NodePropRes(ref a) => live::cmd_live_node_prop_res(a),
        LiveCommand::ResPropRes(ref a) => live::cmd_live_res_prop_res(a),
        LiveCommand::ResCall(ref a) => live::cmd_live_res_call(a),
        LiveCommand::RemoveKeep(ref a) => live::cmd_live_remove_keep(a),
        LiveCommand::Restore(ref a) => live::cmd_live_restore(a),
    }
}

fn exec_scene(args: &SceneGroupArgs) -> Result<()> {
    ensure_binary_debug()?;
    match args.command {
        SceneGroupCommand::Tree(ref a) => scene::cmd_scene_tree(a),
        SceneGroupCommand::Inspect(ref a) => scene::cmd_inspect(a),
        SceneGroupCommand::InspectObjects(ref a) => scene::cmd_inspect_objects(a),
        SceneGroupCommand::CameraView(ref a) => scene::cmd_camera_view(a),
    }
}

fn exec_camera(args: &CameraGroupArgs) -> Result<()> {
    ensure_binary_debug()?;
    match args.command {
        CameraGroupCommand::Override(ref a) => misc::cmd_override_camera(a),
        CameraGroupCommand::Transform2d(ref a) => camera::cmd_transform_camera_2d(a),
        CameraGroupCommand::Transform3d(ref a) => camera::cmd_transform_camera_3d(a),
        CameraGroupCommand::Screenshot(ref a) => camera::cmd_screenshot(a),
    }
}

fn exec_select(args: &SelectArgs) -> Result<()> {
    ensure_binary_debug()?;
    match args.command {
        SelectCommand::Type(ref a) => selection::cmd_node_select_type(a),
        SelectCommand::Mode(ref a) => selection::cmd_node_select_mode(a),
        SelectCommand::Visible(ref a) => selection::cmd_node_select_visible(a),
        SelectCommand::AvoidLocked(ref a) => selection::cmd_node_select_avoid_locked(a),
        SelectCommand::PreferGroup(ref a) => selection::cmd_node_select_prefer_group(a),
        SelectCommand::ResetCam2d(ref a) => selection::cmd_node_select_reset_cam_2d(a),
        SelectCommand::ResetCam3d(ref a) => selection::cmd_node_select_reset_cam_3d(a),
        SelectCommand::Clear(ref a) => selection::cmd_clear_selection(a),
    }
}

// ── Daemon helpers ───────────────────────────────────────────────────

/// Send a command through the daemon, returning the result.
fn daemon_cmd(method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    crate::lsp::daemon_client::query_daemon(method, params, None)
}

/// Send a command through the daemon with a custom timeout.
fn daemon_cmd_timeout(
    method: &str,
    params: serde_json::Value,
    timeout_secs: u64,
) -> Option<serde_json::Value> {
    crate::lsp::daemon_client::query_daemon(
        method,
        params,
        Some(std::time::Duration::from_secs(timeout_secs + 5)),
    )
}

/// Ensure the binary debug server is running and a game is connected.
/// Returns Ok(()) if ready, or a helpful error with instructions.
fn ensure_binary_debug() -> Result<()> {
    // Check current status
    if let Some(status) = daemon_cmd("debug_server_status", serde_json::json!({}))
        && status.get("running").and_then(serde_json::Value::as_bool) == Some(true)
    {
        if status.get("connected").and_then(serde_json::Value::as_bool) == Some(true) {
            return Ok(()); // Already running and connected
        }
        // Server exists but no game connected yet — the async accept from
        // `gd run` may still be waiting. Try a short accept to catch it.
        let port = status
            .get("port")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let accept = daemon_cmd_timeout("debug_accept", serde_json::json!({"timeout": 3}), 8);
        if let Some(r) = accept
            && r.get("connected").and_then(serde_json::Value::as_bool) == Some(true)
        {
            return Ok(());
        }
        return Err(miette!(
            "Debug server running on port {port} but no game is connected.\n\
             Launch your game with: gd run\n\
             Or manually: godot --remote-debug tcp://127.0.0.1:{port}"
        ));
    }

    // No server running — start one
    let result = daemon_cmd("debug_start_server", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to start binary debug server (daemon not available)"))?;
    let port = result
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    // Wait briefly for a connection, then advise
    let accept = daemon_cmd_timeout("debug_accept", serde_json::json!({"timeout": 3}), 8);
    if let Some(r) = accept
        && r.get("connected").and_then(serde_json::Value::as_bool) == Some(true)
    {
        return Ok(());
    }

    Err(miette!(
        "Debug server started on port {port} — waiting for game connection.\n\
         Launch your game with: gd run\n\
         Or manually: godot --remote-debug tcp://127.0.0.1:{port}"
    ))
}

/// Check if the game is currently paused at a breakpoint.
/// Uses the daemon's atomic flag (set by reader thread on debug_enter/debug_exit),
/// so this returns instantly with no network round-trip.
fn is_at_breakpoint() -> bool {
    daemon_cmd("debug_is_at_breakpoint", serde_json::json!({}))
        .and_then(|v| v.get("at_breakpoint")?.as_bool())
        == Some(true)
}

/// Context from auto-breaking for eval. Caller must call `cleanup()` after eval.
struct EvalBreakContext {
    auto_broke: bool,
    temp_breakpoint: Option<(String, u32)>,
}

impl EvalBreakContext {
    fn cleanup(&self) {
        if let Some((ref path, line)) = self.temp_breakpoint {
            daemon_cmd(
                "debug_breakpoint",
                serde_json::json!({"path": path, "line": line, "enabled": false}),
            );
        }
        if self.auto_broke {
            daemon_cmd("debug_continue", serde_json::json!({}));
        }
    }
}

/// Try to enter the debug loop so evaluate works.
/// If already at a breakpoint, returns immediately (no cleanup needed).
/// Otherwise sets a temporary breakpoint on a `_process` function so the game
/// pauses with a real GDScript context (the raw `break` command pauses the
/// engine but does NOT provide a script stack frame, so evaluate fails).
fn debug_break_for_eval() -> EvalBreakContext {
    if is_at_breakpoint() {
        return EvalBreakContext {
            auto_broke: false,
            temp_breakpoint: None,
        };
    }

    // Set a temporary breakpoint on a _process/_physics_process body line.
    // These run every frame so the breakpoint triggers within ~16ms.
    if let Some((res_path, line)) = find_process_breakpoint_target() {
        daemon_cmd(
            "debug_breakpoint",
            serde_json::json!({"path": res_path, "line": line, "enabled": true}),
        );

        // Wait up to 2 seconds for the breakpoint to hit
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if is_at_breakpoint() {
                return EvalBreakContext {
                    auto_broke: true,
                    temp_breakpoint: Some((res_path, line)),
                };
            }
        }

        // Didn't trigger — remove the breakpoint
        daemon_cmd(
            "debug_breakpoint",
            serde_json::json!({"path": res_path, "line": line, "enabled": false}),
        );
    }

    // Fallback: try raw break command (may not provide GDScript context)
    let _ = daemon_cmd("debug_break_exec", serde_json::json!({}));
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if is_at_breakpoint() {
            return EvalBreakContext {
                auto_broke: true,
                temp_breakpoint: None,
            };
        }
    }

    EvalBreakContext {
        auto_broke: true,
        temp_breakpoint: None,
    }
}

/// Scan project .gd files for a `_process` or `_physics_process` function
/// and return a `(res://path, body_line)` pair suitable for a breakpoint.
fn find_process_breakpoint_target() -> Option<(String, u32)> {
    let cwd = std::env::current_dir().ok()?;
    let root = crate::core::config::find_project_root(&cwd)?;
    let files = crate::core::fs::collect_gdscript_files(&root).ok()?;

    for file in &files {
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("func _process(")
                || trimmed.starts_with("func _physics_process(")
            {
                // Find first non-empty, non-comment body line after declaration
                let body_line = content.lines().skip(i + 1).enumerate().find_map(|(j, l)| {
                    let t = l.trim();
                    if !t.is_empty() && !t.starts_with('#') {
                        Some((i + j + 2) as u32) // 1-based
                    } else {
                        None
                    }
                });

                if let Some(body_line) = body_line {
                    let Some(rel) = file.strip_prefix(&root).ok() else {
                        continue;
                    };
                    let res_path = format!("res://{}", rel.to_slash_lossy());
                    return Some((res_path, body_line));
                }
            }
        }
    }
    None
}
