use std::time::Duration;

use crate::debug::variant::GodotVariant;

use super::parsers::{parse_debug_variable, parse_eval_result, parse_stack_dump, parse_var_counts};
use super::parsers::{variant_as_string, variant_as_u32, variant_as_u64};
use super::{EvalResult, FrameVariables, GodotDebugServer, ScreenshotResult, StackFrameInfo};

impl GodotDebugServer {
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

    pub fn cmd_reload_scripts(&self, paths: &[String]) -> bool {
        let args: Vec<GodotVariant> = paths
            .iter()
            .map(|p| GodotVariant::String(p.clone()))
            .collect();
        self.send_command("reload_scripts", &args)
    }

    pub fn cmd_reload_all_scripts(&self) -> bool {
        self.send_command("reload_all_scripts", &[])
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
    // Profiler commands
    // ═══════════════════════════════════════════════════════════════════

    /// Toggle a profiler (scripts, visual, servers).
    pub fn cmd_toggle_profiler(&self, profiler_name: &str, enable: bool) -> bool {
        let cmd = format!("profiler:{profiler_name}");
        self.send_command(&cmd, &[GodotVariant::Bool(enable)])
    }
}
