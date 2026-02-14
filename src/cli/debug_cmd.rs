// DAP (Microsoft Debug Adapter Protocol) command implementations are preserved
// below for future use. They are intentionally unreachable — not exposed via
// CLI. The active debug interface uses Godot's binary debug protocol instead.
#![allow(dead_code)]

use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::debug::{BreakpointResult, Scope, StackFrame, Variable};

#[derive(Args)]
pub struct DebugArgs {
    #[command(subcommand)]
    pub command: DebugCommand,
}

#[derive(Subcommand)]
pub enum DebugCommand {
    // ── Binary debug protocol commands ──
    /// Show the running game's scene tree
    #[command(name = "scene-tree")]
    SceneTree(SceneTreeArgs),
    /// Inspect a scene node's properties by object ID
    Inspect(InspectArgs),
    /// Set a property on a scene node by object ID
    #[command(name = "set-prop")]
    SetProp(SetPropArgs),
    /// Suspend (freeze) the game loop
    Suspend(SuspendArgs),
    /// Advance one physics frame while suspended
    #[command(name = "next-frame")]
    NextFrame(StepArgs),
    /// Set the game's time scale (Engine.time_scale)
    #[command(name = "time-scale")]
    TimeScale(TimeScaleArgs),
    /// Hot-reload all GDScript files in the running game
    #[command(name = "reload-scripts")]
    ReloadScripts(StepArgs),
    /// Reload all scripts in the running game (binary protocol)
    #[command(name = "reload-all-scripts")]
    ReloadAllScripts(StepArgs),
    /// Skip all breakpoints (toggle)
    #[command(name = "skip-breakpoints")]
    SkipBreakpoints(SkipBreakpointsArgs),
    /// Ignore error breaks (toggle)
    #[command(name = "ignore-errors")]
    IgnoreErrors(IgnoreErrorsArgs),
    /// Mute/unmute game audio
    #[command(name = "mute-audio")]
    MuteAudio(MuteAudioArgs),
    /// Override the game camera (take remote control)
    #[command(name = "override-camera")]
    OverrideCamera(OverrideCameraArgs),
    /// Save a scene node to a file
    #[command(name = "save-node")]
    SaveNode(SaveNodeArgs),
    /// Set a specific field within a property (e.g. position.x)
    #[command(name = "set-prop-field")]
    SetPropField(SetPropFieldArgs),
    /// Toggle a profiler (scripts, visual, servers)
    Profiler(ProfilerArgs),
    /// Live editing: set root scene
    #[command(name = "live-set-root")]
    LiveSetRoot(LiveSetRootArgs),
    /// Live editing: create a new node
    #[command(name = "live-create-node")]
    LiveCreateNode(LiveCreateNodeArgs),
    /// Live editing: instantiate a scene
    #[command(name = "live-instantiate")]
    LiveInstantiate(LiveInstantiateArgs),
    /// Live editing: remove a node
    #[command(name = "live-remove-node")]
    LiveRemoveNode(LiveRemoveNodeArgs),
    /// Live editing: duplicate a node
    #[command(name = "live-duplicate")]
    LiveDuplicate(LiveDuplicateArgs),
    /// Live editing: reparent a node
    #[command(name = "live-reparent")]
    LiveReparent(LiveReparentArgs),
    /// Live editing: set a node property
    #[command(name = "live-node-prop")]
    LiveNodeProp(LiveNodePropArgs),
    /// Live editing: call a method on a node
    #[command(name = "live-node-call")]
    LiveNodeCall(LiveNodeCallArgs),

    /// Stop the running game (alias for `gd stop`)
    Stop,

    // ── Execution control ──
    /// Resume execution from breakpoint
    Continue(StepArgs),
    /// Pause/break execution
    Pause(StepArgs),
    /// Step over (next line)
    #[command(visible_alias = "step-over")]
    Next(StepArgs),
    /// Step into function
    #[command(name = "step-in")]
    StepIn(StepArgs),
    /// Step out of function
    #[command(name = "step-out")]
    StepOutFn(StepArgs),

    // ── Debugging ──
    /// Set/clear a breakpoint
    Breakpoint(BreakpointBinArgs),
    /// Get call stack
    Stack(StepArgs),
    /// Get variables for a stack frame
    Vars(VarsArgs),
    /// Evaluate expression
    Eval(EvalBinArgs),

    // ── Multi-object inspection ──
    /// Inspect multiple objects
    #[command(name = "inspect-objects")]
    InspectObjects(InspectObjectsArgs),

    // ── Camera ──
    /// Transform 2D camera
    #[command(name = "transform-camera-2d")]
    TransformCamera2d(TransformCamera2dArgs),
    /// Transform 3D camera
    #[command(name = "transform-camera-3d")]
    TransformCamera3d(TransformCamera3dArgs),

    // ── Screenshot ──
    /// Request screenshot
    Screenshot(ScreenshotArgs),

    // ── File management ──
    /// Reload cached files
    #[command(name = "reload-cached")]
    ReloadCached(ReloadCachedArgs),

    // ── Node selection ──
    /// Set selection type
    #[command(name = "node-select-type")]
    NodeSelectType(NodeSelectIntArgs),
    /// Set selection mode
    #[command(name = "node-select-mode")]
    NodeSelectMode(NodeSelectIntArgs),
    /// Toggle visibility filter
    #[command(name = "node-select-visible")]
    NodeSelectVisible(ToggleFmtArgs),
    /// Toggle avoid locked
    #[command(name = "node-select-avoid-locked")]
    NodeSelectAvoidLocked(ToggleFmtArgs),
    /// Toggle prefer group
    #[command(name = "node-select-prefer-group")]
    NodeSelectPreferGroup(ToggleFmtArgs),
    /// Reset 2D selection camera
    #[command(name = "node-select-reset-cam-2d")]
    NodeSelectResetCam2d(StepArgs),
    /// Reset 3D selection camera
    #[command(name = "node-select-reset-cam-3d")]
    NodeSelectResetCam3d(StepArgs),
    /// Clear selection
    #[command(name = "clear-selection")]
    ClearSelection(StepArgs),

    // ── Live editing: resource operations ──
    /// Live editing: set node path mapping
    #[command(name = "live-node-path")]
    LiveNodePath(LivePathArgs),
    /// Live editing: set resource path mapping
    #[command(name = "live-res-path")]
    LiveResPath(LivePathArgs),
    /// Live editing: set resource property
    #[command(name = "live-res-prop")]
    LiveResProp(LiveNodePropArgs),
    /// Live editing: set node property to resource
    #[command(name = "live-node-prop-res")]
    LiveNodePropRes(LivePropResArgs),
    /// Live editing: set resource property to resource
    #[command(name = "live-res-prop-res")]
    LiveResPropRes(LivePropResArgs),
    /// Live editing: call method on resource
    #[command(name = "live-res-call")]
    LiveResCall(LiveNodeCallArgs),

    // ── Live editing: advanced node operations ──
    /// Live editing: remove node but keep reference
    #[command(name = "live-remove-keep")]
    LiveRemoveKeep(LiveRemoveKeepArgs),
    /// Live editing: restore previously removed node
    #[command(name = "live-restore")]
    LiveRestore(LiveRestoreArgs),

    /// Start the binary debug server and print the port (for manual testing)
    Server(ServerArgs),
}

#[derive(Args)]
pub struct BreakArgs {
    /// Script file path (relative to project root, e.g. scripts/kart.gd)
    #[arg(long)]
    pub file: Option<String>,
    /// Line numbers to set breakpoints on
    #[arg(long, num_args = 1..)]
    pub line: Vec<u32>,
    /// Function name to break on (resolves to file:line automatically)
    #[arg(long)]
    pub name: Option<String>,
    /// Condition expression (breakpoint only triggers when true)
    #[arg(long)]
    pub condition: Option<String>,
    /// Timeout in seconds to wait for breakpoint hit (default: 30)
    #[arg(long, default_value = "30")]
    pub timeout: u64,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct EvalArgs {
    /// Expression to evaluate (e.g. "self.speed", "position.x")
    #[arg(long)]
    pub expr: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SetVarArgs {
    /// Variable name to set (member variable, e.g. "speed", "max_health")
    #[arg(long)]
    pub name: String,
    /// New value (as string, e.g. "3.0", "true", "Vector3(1,2,3)")
    #[arg(long)]
    pub value: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct StepArgs {
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SceneTreeArgs {
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct InspectArgs {
    /// Object ID to inspect (from scene-tree output)
    #[arg(long)]
    pub id: u64,
    /// Brief output: just name=value pairs, no Godot internals
    #[arg(long)]
    pub brief: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SetPropArgs {
    /// Object ID
    #[arg(long)]
    pub id: u64,
    /// Property name
    #[arg(long)]
    pub property: String,
    /// New value (JSON: numbers, strings, booleans, null)
    #[arg(long)]
    pub value: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SuspendArgs {
    /// Resume instead of suspend
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct TimeScaleArgs {
    /// Time scale (1.0 = normal, 0.5 = half speed, 2.0 = double speed)
    #[arg(long)]
    pub scale: f64,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SkipBreakpointsArgs {
    /// Disable skipping (re-enable breakpoints)
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct IgnoreErrorsArgs {
    /// Stop ignoring errors
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct MuteAudioArgs {
    /// Unmute instead
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct OverrideCameraArgs {
    /// Disable camera override
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SaveNodeArgs {
    /// Object ID of the node to save
    #[arg(long)]
    pub id: u64,
    /// File path to save to (on game's filesystem)
    #[arg(long)]
    pub path: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SetPropFieldArgs {
    /// Object ID
    #[arg(long)]
    pub id: u64,
    /// Property name (e.g. "position")
    #[arg(long)]
    pub property: String,
    /// Field name (e.g. "x")
    #[arg(long)]
    pub field: String,
    /// New value (JSON)
    #[arg(long)]
    pub value: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ProfilerArgs {
    /// Profiler name: scripts, visual, or servers
    #[arg(long)]
    pub name: String,
    /// Disable instead of enable
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveSetRootArgs {
    /// Scene path (e.g. "/root/Main")
    #[arg(long)]
    pub path: String,
    /// Scene file (e.g. "res://main.tscn")
    #[arg(long)]
    pub file: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveCreateNodeArgs {
    /// Parent node path
    #[arg(long)]
    pub parent: String,
    /// Node class (e.g. "Sprite2D")
    #[arg(long, name = "class")]
    pub class_name: String,
    /// Node name
    #[arg(long)]
    pub name: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveInstantiateArgs {
    /// Parent node path
    #[arg(long)]
    pub parent: String,
    /// Scene resource path (e.g. "res://enemy.tscn")
    #[arg(long)]
    pub scene: String,
    /// Instance name
    #[arg(long)]
    pub name: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveRemoveNodeArgs {
    /// Node path to remove
    #[arg(long)]
    pub path: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveDuplicateArgs {
    /// Node path to duplicate
    #[arg(long)]
    pub path: String,
    /// Name for the duplicate
    #[arg(long)]
    pub name: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveReparentArgs {
    /// Node path to reparent
    #[arg(long)]
    pub path: String,
    /// New parent path
    #[arg(long)]
    pub new_parent: String,
    /// New name (empty = keep same name)
    #[arg(long, default_value = "")]
    pub name: String,
    /// Position in parent's children (-1 = end)
    #[arg(long, default_value = "-1")]
    pub pos: i32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveNodePropArgs {
    /// Live edit node ID (from live-set-root mapping)
    #[arg(long)]
    pub id: i32,
    /// Property name
    #[arg(long)]
    pub property: String,
    /// Value (JSON)
    #[arg(long)]
    pub value: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveNodeCallArgs {
    /// Live edit node ID
    #[arg(long)]
    pub id: i32,
    /// Method name
    #[arg(long)]
    pub method: String,
    /// Arguments as JSON array (e.g. '[1, "hello", true]')
    #[arg(long, default_value = "[]")]
    pub args: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct BreakpointBinArgs {
    /// Script file path (e.g. res://scripts/kart.gd)
    #[arg(long)]
    pub path: Option<String>,
    /// Line number
    #[arg(long)]
    pub line: Option<u32>,
    /// Function name — resolves to file:line automatically (e.g. "_process", "take_damage")
    #[arg(long)]
    pub name: Option<String>,
    /// Condition expression — breakpoint only triggers when this evaluates to true
    #[arg(long)]
    pub condition: Option<String>,
    /// Disable (clear) this breakpoint
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct VarsArgs {
    /// Stack frame index (default: 0 = top frame)
    #[arg(long, default_value = "0")]
    pub frame: u32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct EvalBinArgs {
    /// Expression to evaluate
    #[arg(long)]
    pub expr: String,
    /// Stack frame index (default: 0 = top frame)
    #[arg(long, default_value = "0")]
    pub frame: u32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct InspectObjectsArgs {
    /// Object IDs to inspect (repeatable)
    #[arg(long, num_args = 1..)]
    pub id: Vec<u64>,
    /// Inspect the current editor selection
    #[arg(long)]
    pub selection: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct TransformCamera2dArgs {
    /// Transform as JSON array of 6 floats
    #[arg(long)]
    pub transform: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct TransformCamera3dArgs {
    /// Transform as JSON array of 12 floats
    #[arg(long)]
    pub transform: String,
    /// Use perspective projection (default: true)
    #[arg(long, default_value = "true")]
    pub perspective: bool,
    /// Field of view
    #[arg(long)]
    pub fov: f64,
    /// Near clip plane
    #[arg(long)]
    pub near: f64,
    /// Far clip plane
    #[arg(long)]
    pub far: f64,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ScreenshotArgs {
    /// Output file path (default: screenshot.png in current directory)
    #[arg(long, short, default_value = "screenshot.png")]
    pub output: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ReloadCachedArgs {
    /// File paths to reload (repeatable)
    #[arg(long, num_args = 1..)]
    pub file: Vec<String>,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct NodeSelectIntArgs {
    /// Integer value
    #[arg(long)]
    pub value: i32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ToggleFmtArgs {
    /// Disable instead of enable
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LivePathArgs {
    /// Node/resource path
    #[arg(long)]
    pub path: String,
    /// Live edit ID
    #[arg(long)]
    pub id: i32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LivePropResArgs {
    /// Live edit node/resource ID
    #[arg(long)]
    pub id: i32,
    /// Property name
    #[arg(long)]
    pub property: String,
    /// Resource path (e.g. "res://texture.png")
    #[arg(long)]
    pub res_path: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveRemoveKeepArgs {
    /// Node path to remove
    #[arg(long)]
    pub path: String,
    /// Object ID to keep reference to
    #[arg(long)]
    pub object_id: u64,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveRestoreArgs {
    /// Object ID of the previously removed node
    #[arg(long)]
    pub object_id: u64,
    /// Path to restore the node at
    #[arg(long)]
    pub path: String,
    /// Position in parent's children (-1 = end)
    #[arg(long, default_value = "-1")]
    pub pos: i32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ServerArgs {
    /// Port to listen on (default: 6008)
    #[arg(long, default_value = "6008")]
    pub port: u16,
    /// Wait for a game to connect (blocks until connection or timeout)
    #[arg(long)]
    pub wait: bool,
    /// Timeout in seconds when using --wait (default: 60)
    #[arg(long, default_value = "60")]
    pub timeout: u64,
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Human,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown format: {other}")),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Human => write!(f, "human"),
            Self::Json => write!(f, "json"),
        }
    }
}

pub fn exec(args: DebugArgs) -> Result<()> {
    match args.command {
        DebugCommand::Stop => crate::cli::stop_cmd::exec(),
        DebugCommand::SceneTree(a) => cmd_scene_tree(a),
        DebugCommand::Inspect(a) => cmd_inspect(a),
        DebugCommand::SetProp(a) => cmd_set_prop(a),
        DebugCommand::Suspend(a) => cmd_suspend(a),
        DebugCommand::NextFrame(a) => cmd_next_frame(a),
        DebugCommand::TimeScale(a) => cmd_time_scale(a),
        DebugCommand::ReloadScripts(a) => cmd_reload_scripts(a),
        DebugCommand::ReloadAllScripts(a) => cmd_reload_all_scripts(a),
        DebugCommand::SkipBreakpoints(a) => cmd_skip_breakpoints(a),
        DebugCommand::IgnoreErrors(a) => cmd_ignore_errors(a),
        DebugCommand::MuteAudio(a) => cmd_mute_audio(a),
        DebugCommand::OverrideCamera(a) => cmd_override_camera(a),
        DebugCommand::SaveNode(a) => cmd_save_node(a),
        DebugCommand::SetPropField(a) => cmd_set_prop_field(a),
        DebugCommand::Profiler(a) => cmd_profiler(a),
        DebugCommand::LiveSetRoot(a) => cmd_live_set_root(a),
        DebugCommand::LiveCreateNode(a) => cmd_live_create_node(a),
        DebugCommand::LiveInstantiate(a) => cmd_live_instantiate(a),
        DebugCommand::LiveRemoveNode(a) => cmd_live_remove_node(a),
        DebugCommand::LiveDuplicate(a) => cmd_live_duplicate(a),
        DebugCommand::LiveReparent(a) => cmd_live_reparent(a),
        DebugCommand::LiveNodeProp(a) => cmd_live_node_prop(a),
        DebugCommand::LiveNodeCall(a) => cmd_live_node_call(a),
        DebugCommand::Continue(a) => cmd_exec_continue(a),
        DebugCommand::Pause(a) => cmd_exec_pause(a),
        DebugCommand::Next(a) => cmd_exec_next(a),
        DebugCommand::StepIn(a) => cmd_exec_step_in(a),
        DebugCommand::StepOutFn(a) => cmd_exec_step_out(a),
        DebugCommand::Breakpoint(a) => cmd_breakpoint(a),
        DebugCommand::Stack(a) => cmd_stack(a),
        DebugCommand::Vars(a) => cmd_vars(a),
        DebugCommand::Eval(a) => cmd_evaluate(a),
        DebugCommand::InspectObjects(a) => cmd_inspect_objects(a),
        DebugCommand::TransformCamera2d(a) => cmd_transform_camera_2d(a),
        DebugCommand::TransformCamera3d(a) => cmd_transform_camera_3d(a),
        DebugCommand::Screenshot(a) => cmd_screenshot(a),
        DebugCommand::ReloadCached(a) => cmd_reload_cached(a),
        DebugCommand::NodeSelectType(a) => cmd_node_select_type(a),
        DebugCommand::NodeSelectMode(a) => cmd_node_select_mode(a),
        DebugCommand::NodeSelectVisible(a) => cmd_node_select_visible(a),
        DebugCommand::NodeSelectAvoidLocked(a) => cmd_node_select_avoid_locked(a),
        DebugCommand::NodeSelectPreferGroup(a) => cmd_node_select_prefer_group(a),
        DebugCommand::NodeSelectResetCam2d(a) => cmd_node_select_reset_cam_2d(a),
        DebugCommand::NodeSelectResetCam3d(a) => cmd_node_select_reset_cam_3d(a),
        DebugCommand::ClearSelection(a) => cmd_clear_selection(a),
        DebugCommand::LiveNodePath(a) => cmd_live_node_path(a),
        DebugCommand::LiveResPath(a) => cmd_live_res_path(a),
        DebugCommand::LiveResProp(a) => cmd_live_res_prop(a),
        DebugCommand::LiveNodePropRes(a) => cmd_live_node_prop_res(a),
        DebugCommand::LiveResPropRes(a) => cmd_live_res_prop_res(a),
        DebugCommand::LiveResCall(a) => cmd_live_res_call(a),
        DebugCommand::LiveRemoveKeep(a) => cmd_live_remove_keep(a),
        DebugCommand::LiveRestore(a) => cmd_live_restore(a),
        DebugCommand::Server(a) => cmd_server(a),
    }
}

// ── Daemon helpers ───────────────────────────────────────────────────

/// Send a DAP method through the daemon, returning the result.
fn daemon_dap(method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    crate::lsp::daemon_client::query_daemon(method, params, None)
}

/// Send a DAP method through the daemon with a custom timeout.
fn daemon_dap_timeout(
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
    if let Some(status) = daemon_dap("debug_server_status", serde_json::json!({}))
        && status.get("running").and_then(|r| r.as_bool()) == Some(true)
    {
        if status.get("connected").and_then(|c| c.as_bool()) == Some(true) {
            return Ok(()); // Already running and connected
        }
        let port = status.get("port").and_then(|p| p.as_u64()).unwrap_or(0);
        return Err(miette!(
            "Debug server running on port {port} but no game is connected.\n\
             Launch your game with: gd run --debug\n\
             Or manually: godot --remote-debug tcp://127.0.0.1:{port}"
        ));
    }

    // Start the debug server
    let result = daemon_dap("debug_start_server", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to start binary debug server (daemon not available)"))?;
    let port = result.get("port").and_then(|p| p.as_u64()).unwrap_or(0);

    // Wait briefly for a connection, then advise
    let accept = daemon_dap_timeout("debug_accept", serde_json::json!({"timeout": 2}), 5);
    if let Some(r) = accept
        && r.get("connected").and_then(|c| c.as_bool()) == Some(true)
    {
        return Ok(());
    }

    Err(miette!(
        "Debug server started on port {port} — waiting for game connection.\n\
         Launch your game with: gd run --debug\n\
         Or manually: godot --remote-debug tcp://127.0.0.1:{port}"
    ))
}

/// Resolve a relative script path using the daemon's project path.
fn resolve_script_path(relative: &str) -> Option<String> {
    // Verify file exists locally
    let cwd = std::env::current_dir().ok()?;
    let project = crate::core::project::GodotProject::discover(&cwd).ok()?;
    let full = project.root.join(relative);
    if !full.exists() {
        return None;
    }

    let result = daemon_dap("dap_project_path", serde_json::json!({}))?;
    let editor_root = result.get("project_path")?.as_str()?;
    let relative_fwd = relative.replace('\\', "/");
    Some(format!("{editor_root}/{relative_fwd}"))
}

// ── Server command ───────────────────────────────────────────────────

fn cmd_server(args: ServerArgs) -> Result<()> {
    // Check if already running
    if let Some(status) = daemon_dap("debug_server_status", serde_json::json!({}))
        && status.get("running").and_then(|r| r.as_bool()) == Some(true)
    {
        let port = status.get("port").and_then(|p| p.as_u64()).unwrap_or(0);
        let connected = status.get("connected").and_then(|c| c.as_bool()) == Some(true);
        if connected {
            println!(
                "{} port {} (game connected)",
                "Debug server already running on".green().bold(),
                port.to_string().cyan(),
            );
            return Ok(());
        }
        println!(
            "{} port {} (waiting for game)",
            "Debug server already running on".yellow().bold(),
            port.to_string().cyan(),
        );
        if !args.wait {
            print_launch_hint(port);
            return Ok(());
        }
        // Fall through to wait
        let accept = daemon_dap_timeout(
            "debug_accept",
            serde_json::json!({"timeout": args.timeout}),
            args.timeout + 5,
        );
        if let Some(r) = accept
            && r.get("connected").and_then(|c| c.as_bool()) == Some(true)
        {
            println!("{}", "Game connected!".green().bold());
            return Ok(());
        }
        return Err(miette!("Timed out waiting for game to connect"));
    }

    // Start the server
    let result = daemon_dap("debug_start_server", serde_json::json!({"port": args.port}))
        .ok_or_else(|| miette!("Failed to start debug server (daemon not available)"))?;
    let port = result.get("port").and_then(|p| p.as_u64()).unwrap_or(0);

    println!(
        "{} port {}",
        "Debug server started on".green().bold(),
        port.to_string().cyan(),
    );
    print_launch_hint(port);

    if args.wait {
        println!("{}", "Waiting for game to connect...".dimmed());
        let accept = daemon_dap_timeout(
            "debug_accept",
            serde_json::json!({"timeout": args.timeout}),
            args.timeout + 5,
        );
        if let Some(r) = accept
            && r.get("connected").and_then(|c| c.as_bool()) == Some(true)
        {
            println!("{}", "Game connected!".green().bold());
            return Ok(());
        }
        return Err(miette!("Timed out waiting for game to connect"));
    }

    Ok(())
}

/// Print a copy-pasteable Godot launch command with platform-correct project path.
fn print_launch_hint(port: u64) {
    let project_path = std::env::current_dir()
        .ok()
        .and_then(|cwd| crate::core::config::find_project_root(&cwd))
        .map(|root| {
            let s = root.to_string_lossy();
            crate::core::fs::wsl_to_windows_path(&s).unwrap_or_else(|| s.to_string())
        });

    if let Some(path) = project_path {
        println!(
            "Launch game with:\n  {} {} {} {}",
            "godot".bold(),
            format!("--remote-debug \"tcp://127.0.0.1:{port}\"").cyan(),
            "--path".bold(),
            format!("\"{path}\"").cyan(),
        );
    } else {
        println!(
            "Launch game with:\n  {} {}",
            "godot".bold(),
            format!("--remote-debug \"tcp://127.0.0.1:{port}\"").cyan(),
        );
    }
}

// ── Interactive session ──────────────────────────────────────────────

fn cmd_attach() -> Result<()> {
    // Verify daemon is available
    daemon_dap("dap_status", serde_json::json!({})).ok_or_else(|| {
        miette!("Could not connect to Godot DAP via daemon\n  Is the Godot editor running?")
    })?;

    println!(
        "{} {}",
        "Attached to Godot DAP".green().bold(),
        "(via daemon)".dimmed(),
    );
    println!(
        "Type {} for commands, {} to exit.\n",
        "help".cyan(),
        "quit".cyan()
    );

    let stdin = std::io::stdin();
    let mut line = String::new();

    loop {
        eprint!("{} ", "gd>".green().bold());

        line.clear();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "help" | "h" => print_help(),
            "quit" | "q" | "exit" => break,
            "continue" | "c" => {
                if daemon_dap("dap_continue", serde_json::json!({})).is_some() {
                    println!("{}", "Continued".green());
                } else {
                    println!("{}", "Failed to continue".red());
                }
            }
            "pause" | "p" => {
                if daemon_dap("dap_pause", serde_json::json!({})).is_some() {
                    println!("{}", "Paused".green());
                } else {
                    println!("{}", "Failed to pause".red());
                }
            }
            "next" | "n" => {
                if daemon_dap("dap_next", serde_json::json!({})).is_some() {
                    println!("{}", "Stepped over".green());
                } else {
                    println!("{}", "Failed to step over".red());
                }
            }
            "step" | "s" => {
                if daemon_dap("dap_step_in", serde_json::json!({})).is_some() {
                    println!("{}", "Stepped in".green());
                } else {
                    println!("{}", "Failed to step in".red());
                }
            }
            "out" | "o" => repl_step_out(),
            "stack" | "bt" => repl_stack(),
            "vars" => repl_vars(args.first().copied()),
            "expand" => {
                if let Some(ref_str) = args.first() {
                    if let Ok(vref) = ref_str.parse::<i64>() {
                        repl_expand(vref);
                    } else {
                        println!("Usage: expand <ref_id>");
                    }
                } else {
                    println!("Usage: expand <ref_id>");
                }
            }
            "eval" | "e" => {
                if args.is_empty() {
                    println!("Usage: eval <expression>");
                } else {
                    let expr = args.join(" ");
                    repl_eval(&expr);
                }
            }
            "break" | "b" => {
                if args.len() < 2 {
                    println!("Usage: break <file> <line> [line2 ...]");
                } else {
                    repl_break(args[0], &args[1..]);
                }
            }
            "clear" => {
                if args.is_empty() {
                    println!("Usage: clear <file>");
                } else {
                    repl_clear(args[0]);
                }
            }
            "wait" => {
                let timeout = args
                    .first()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(30);
                repl_wait(timeout);
            }
            "scene-tree" | "tree" => repl_scene_tree(),
            "inspect" | "i" => {
                if let Some(id_str) = args.first() {
                    if let Ok(id) = id_str.parse::<u64>() {
                        repl_inspect(id);
                    } else {
                        println!("Usage: inspect <object_id>");
                    }
                } else {
                    println!("Usage: inspect <object_id>");
                }
            }
            "set-prop" => {
                if args.len() >= 3 {
                    if let Ok(id) = args[0].parse::<u64>() {
                        let prop = args[1];
                        let val = args[2..].join(" ");
                        repl_set_prop(id, prop, &val);
                    } else {
                        println!("Usage: set-prop <object_id> <property> <value>");
                    }
                } else {
                    println!("Usage: set-prop <object_id> <property> <value>");
                }
            }
            "suspend" => repl_suspend(true),
            "resume" => repl_suspend(false),
            "next-frame" | "nf" => repl_next_frame(),
            "timescale" => {
                if let Some(scale_str) = args.first() {
                    if let Ok(scale) = scale_str.parse::<f64>() {
                        repl_time_scale(scale);
                    } else {
                        println!("Usage: timescale <N>");
                    }
                } else {
                    println!("Usage: timescale <N>");
                }
            }
            "reload" => repl_reload_scripts(),
            "reload-all" => repl_reload_all_scripts(),
            "skip-bp" => repl_skip_breakpoints(false),
            "unskip-bp" => repl_skip_breakpoints(true),
            "ignore-errors" => repl_ignore_errors(false),
            "unignore-errors" => repl_ignore_errors(true),
            "mute" => repl_mute_audio(true),
            "unmute" => repl_mute_audio(false),
            "override-cam" => repl_override_camera(false),
            "no-override-cam" => repl_override_camera(true),
            "save-node" => {
                if args.len() >= 2 {
                    if let Ok(id) = args[0].parse::<u64>() {
                        repl_save_node(id, args[1]);
                    } else {
                        println!("Usage: save-node <object_id> <path>");
                    }
                } else {
                    println!("Usage: save-node <object_id> <path>");
                }
            }
            "set-prop-field" => {
                if args.len() >= 4 {
                    if let Ok(id) = args[0].parse::<u64>() {
                        let val = args[3..].join(" ");
                        repl_set_prop_field(id, args[1], args[2], &val);
                    } else {
                        println!("Usage: set-prop-field <id> <property> <field> <value>");
                    }
                } else {
                    println!("Usage: set-prop-field <id> <property> <field> <value>");
                }
            }
            "profiler" => {
                if let Some(name) = args.first() {
                    let off = args.get(1).is_some_and(|s| *s == "off");
                    repl_profiler(name, off);
                } else {
                    println!("Usage: profiler <scripts|visual|servers> [off]");
                }
            }
            "live-root" => {
                if args.len() >= 2 {
                    repl_live_set_root(args[0], args[1]);
                } else {
                    println!("Usage: live-root <path> <file>");
                }
            }
            "live-create" => {
                if args.len() >= 3 {
                    repl_live_create_node(args[0], args[1], args[2]);
                } else {
                    println!("Usage: live-create <parent> <class> <name>");
                }
            }
            "live-inst" => {
                if args.len() >= 3 {
                    repl_live_instantiate(args[0], args[1], args[2]);
                } else {
                    println!("Usage: live-inst <parent> <scene> <name>");
                }
            }
            "live-remove" => {
                if let Some(path) = args.first() {
                    repl_live_remove_node(path);
                } else {
                    println!("Usage: live-remove <path>");
                }
            }
            "live-dup" => {
                if args.len() >= 2 {
                    repl_live_duplicate(args[0], args[1]);
                } else {
                    println!("Usage: live-dup <path> <name>");
                }
            }
            "live-reparent" => {
                if args.len() >= 2 {
                    let name = if args.len() >= 3 { args[2] } else { "" };
                    let pos = args
                        .get(3)
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(-1);
                    repl_live_reparent(args[0], args[1], name, pos);
                } else {
                    println!("Usage: live-reparent <path> <parent> [name] [pos]");
                }
            }
            "live-prop" => {
                if args.len() >= 3 {
                    if let Ok(id) = args[0].parse::<i32>() {
                        let val = args[2..].join(" ");
                        repl_live_node_prop(id, args[1], &val);
                    } else {
                        println!("Usage: live-prop <id> <property> <value>");
                    }
                } else {
                    println!("Usage: live-prop <id> <property> <value>");
                }
            }
            "live-call" => {
                if args.len() >= 2 {
                    if let Ok(id) = args[0].parse::<i32>() {
                        let call_args = if args.len() >= 3 {
                            args[2..].join(" ")
                        } else {
                            "[]".to_string()
                        };
                        repl_live_node_call(id, args[1], &call_args);
                    } else {
                        println!("Usage: live-call <id> <method> [args...]");
                    }
                } else {
                    println!("Usage: live-call <id> <method> [args...]");
                }
            }
            _ => println!("Unknown command: {}. Type 'help' for commands.", cmd.red()),
        }
    }

    println!("{}", "Disconnected.".dimmed());
    Ok(())
}

fn print_help() {
    println!("{}", "Commands:".bold());
    println!(
        "  {} {}         Set breakpoint(s)",
        "break".cyan(),
        "<file> <line> [line2 ...]".dimmed()
    );
    println!(
        "  {} {}              Clear breakpoints in file",
        "clear".cyan(),
        "<file>".dimmed()
    );
    println!(
        "  {} {}            Wait for breakpoint hit",
        "wait".cyan(),
        "[timeout_secs]".dimmed()
    );
    println!(
        "  {} / {}              Continue execution",
        "continue".cyan(),
        "c".dimmed()
    );
    println!(
        "  {} / {}                 Pause execution",
        "pause".cyan(),
        "p".dimmed()
    );
    println!(
        "  {} / {}                  Step over (next line)",
        "next".cyan(),
        "n".dimmed()
    );
    println!(
        "  {} / {}                  Step into",
        "step".cyan(),
        "s".dimmed()
    );
    println!(
        "  {} / {}                   Step out of function",
        "out".cyan(),
        "o".dimmed()
    );
    println!(
        "  {} / {}              Show call stack",
        "stack".cyan(),
        "bt".dimmed()
    );
    println!(
        "  {} {}          Show variables",
        "vars".cyan(),
        "[locals|members|globals]".dimmed()
    );
    println!(
        "  {} {}            Expand nested variable",
        "expand".cyan(),
        "<ref_id>".dimmed()
    );
    println!(
        "  {} {}              Evaluate expression",
        "eval".cyan(),
        "<expr>".dimmed()
    );
    println!(
        "  {} / {}                  Disconnect and exit",
        "quit".cyan(),
        "q".dimmed()
    );
    println!();
    println!("{}", "Binary debug protocol:".bold());
    println!(
        "  {} / {}          Show scene tree",
        "scene-tree".cyan(),
        "tree".dimmed()
    );
    println!(
        "  {} {}          Inspect node by object ID",
        "inspect".cyan(),
        "<id>".dimmed()
    );
    println!(
        "  {} {} Set a node property",
        "set-prop".cyan(),
        "<id> <prop> <val>".dimmed()
    );
    println!("  {}                  Freeze game loop", "suspend".cyan());
    println!("  {}                   Resume game loop", "resume".cyan());
    println!(
        "  {} / {}       Advance one frame (while suspended)",
        "next-frame".cyan(),
        "nf".dimmed()
    );
    println!(
        "  {} {}         Set Engine.time_scale",
        "timescale".cyan(),
        "<N>".dimmed()
    );
    println!("  {}                   Hot-reload scripts", "reload".cyan());
    println!(
        "  {}               Reload all scripts (binary protocol)",
        "reload-all".cyan()
    );
    println!(
        "  {}                  Toggle breakpoint skipping",
        "skip-bp".cyan()
    );
    println!(
        "  {}                Undo breakpoint skipping",
        "unskip-bp".cyan()
    );
    println!(
        "  {}            Toggle error break ignoring",
        "ignore-errors".cyan()
    );
    println!(
        "  {}          Undo error break ignoring",
        "unignore-errors".cyan()
    );
    println!("  {}                     Mute game audio", "mute".cyan());
    println!("  {}                   Unmute game audio", "unmute".cyan());
    println!(
        "  {}             Enable camera override",
        "override-cam".cyan()
    );
    println!(
        "  {}          Disable camera override",
        "no-override-cam".cyan()
    );
    println!(
        "  {} {} Save node to file",
        "save-node".cyan(),
        "<id> <path>".dimmed()
    );
    println!(
        "  {} {} Set prop field",
        "set-prop-field".cyan(),
        "<id> <prop> <field> <val>".dimmed()
    );
    println!(
        "  {} {} Toggle profiler",
        "profiler".cyan(),
        "<name> [off]".dimmed()
    );
    println!();
    println!("{}", "Live editing:".bold());
    println!(
        "  {} {} Set live edit root scene",
        "live-root".cyan(),
        "<path> <file>".dimmed()
    );
    println!(
        "  {} {} Create node",
        "live-create".cyan(),
        "<parent> <class> <name>".dimmed()
    );
    println!(
        "  {} {} Instantiate scene",
        "live-inst".cyan(),
        "<parent> <scene> <name>".dimmed()
    );
    println!(
        "  {} {}       Remove node",
        "live-remove".cyan(),
        "<path>".dimmed()
    );
    println!(
        "  {} {} Duplicate node",
        "live-dup".cyan(),
        "<path> <name>".dimmed()
    );
    println!(
        "  {} {} Reparent node",
        "live-reparent".cyan(),
        "<path> <parent> [name] [pos]".dimmed()
    );
    println!(
        "  {} {} Set node property",
        "live-prop".cyan(),
        "<id> <prop> <val>".dimmed()
    );
    println!(
        "  {} {} Call method",
        "live-call".cyan(),
        "<id> <method> [args...]".dimmed()
    );
}

// ── One-shot: break ──────────────────────────────────────────────────

fn cmd_break(args: BreakArgs) -> Result<()> {
    // Resolve --name to file:line if provided
    let (file, lines) = if let Some(ref func_name) = args.name {
        let (resolved_file, resolved_line) =
            resolve_function_name(func_name, args.file.as_deref())?;
        let lines = if args.line.is_empty() {
            vec![resolved_line]
        } else {
            args.line.clone()
        };
        (resolved_file, lines)
    } else {
        let file = args
            .file
            .as_ref()
            .ok_or_else(|| miette!("--file is required when not using --name"))?
            .clone();
        if args.line.is_empty() {
            return Err(miette!(
                "At least one --line is required when not using --name"
            ));
        }
        (file, args.line.clone())
    };

    // Resolve path using daemon's project path
    let path = resolve_script_path(&file)
        .ok_or_else(|| miette!("Cannot resolve script path — is the daemon connected to Godot?"))?;

    let lines_json: Vec<serde_json::Value> = lines.iter().map(|&l| serde_json::json!(l)).collect();

    // Set breakpoints (don't send condition to Godot — it ignores it)
    let bp_body = daemon_dap(
        "dap_set_breakpoints",
        serde_json::json!({"path": path, "lines": lines_json}),
    )
    .ok_or_else(|| miette!("Failed to set breakpoints — is Godot editor running?"))?;

    let results = parse_breakpoint_results(&bp_body);

    for bp in &results {
        let status = if bp.verified {
            "verified".green().to_string()
        } else {
            "unverified".yellow().to_string()
        };
        println!(
            "  {} {}:{} [{}]",
            "Breakpoint".bold(),
            file.cyan(),
            bp.line,
            status,
        );
    }
    if let Some(ref cond) = args.condition {
        println!("  {} {}", "Condition:".dimmed(), cond.cyan(),);
    }

    // Continue execution
    daemon_dap("dap_continue", serde_json::json!({}));

    println!(
        "\n{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        args.timeout,
    );

    // Wait for stopped event — with client-side condition evaluation
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(args.timeout);

    loop {
        let remaining = deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_secs()
            .max(1);

        let stopped = daemon_dap_timeout(
            "dap_wait_stopped",
            serde_json::json!({"timeout": remaining}),
            remaining,
        );

        if stopped.is_none() {
            return Err(miette!(
                "Timeout — breakpoint was not hit within {}s",
                args.timeout
            ));
        }

        // Client-side condition check
        if let Some(ref cond) = args.condition {
            // Brief pause for scope data
            std::thread::sleep(std::time::Duration::from_millis(200));

            let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
            let eval_result = daemon_dap(
                "dap_evaluate",
                serde_json::json!({
                    "expression": cond,
                    "context": "repl",
                    "frame_id": frame_id,
                }),
            );

            let is_falsy = eval_result
                .as_ref()
                .and_then(|v| v["result"].as_str())
                .is_none_or(|r| {
                    matches!(
                        r,
                        "false" | "False" | "0" | "0.0" | "" | "null" | "Null" | "<null>"
                    )
                });

            if is_falsy {
                // Condition not met — resume and wait again
                daemon_dap("dap_continue", serde_json::json!({}));
                if std::time::Instant::now() >= deadline {
                    return Err(miette!(
                        "Timeout — breakpoint hit but condition `{}` was never true within {}s",
                        cond,
                        args.timeout,
                    ));
                }
                continue;
            }
        }

        break; // Breakpoint hit (and condition met if any)
    }

    println!("{}", "Breakpoint hit!".green().bold());

    // Wait for Godot's debugger to populate scope data (too fast → scope_list errors)
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Get stack frames
    let frames = get_stack_frames();

    // Get variables
    let mut all_vars: Vec<(String, Vec<Variable>)> = Vec::new();
    if let Some(frame_id) = frames.first().map(|f| f.id)
        && let Some(scopes_body) =
            daemon_dap("dap_scopes", serde_json::json!({"frame_id": frame_id}))
        && let Some(scopes) = scopes_body["scopes"].as_array()
    {
        for scope in scopes {
            let name = scope["name"].as_str().unwrap_or("?").to_string();
            let vref = scope["variablesReference"].as_i64().unwrap_or(0);
            if vref > 0
                && let Some(vbody) = daemon_dap(
                    "dap_variables",
                    serde_json::json!({"variables_reference": vref}),
                )
            {
                all_vars.push((name, parse_variables(&vbody)));
            }
        }
    }

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "breakpoints": results,
                "stackFrames": frames,
                "variables": all_vars.iter().map(|(name, vars)| {
                    serde_json::json!({"scope": name, "variables": vars})
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Human => {
            if !frames.is_empty() {
                println!("\n{}", "Call stack:".bold());
                for (i, f) in frames.iter().enumerate() {
                    println!(
                        "  {} {} ({}:{})",
                        format!("#{i}").dimmed(),
                        f.name.green().bold(),
                        f.file.cyan(),
                        f.line,
                    );
                }
            }
            for (scope_name, vars) in &all_vars {
                let _ = print_variables(vars, &OutputFormat::Human, Some(scope_name));
            }
        }
    }

    // Resume execution after inspecting the breakpoint
    daemon_dap("dap_continue", serde_json::json!({}));

    Ok(())
}

// ── One-shot: status ────────────────────────────────────────────────

fn cmd_status(args: StatusArgs) -> Result<()> {
    let result = daemon_dap("dap_status", serde_json::json!({})).ok_or_else(|| {
        miette!("Could not connect to Godot DAP via daemon\n  Is the Godot editor running?")
    })?;

    match args.format {
        OutputFormat::Json => {
            let status = serde_json::json!({
                "connected": true,
                "capabilities": result.get("capabilities"),
                "threads": result.get("threads"),
            });
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
        }
        OutputFormat::Human => {
            println!(
                "{} {}",
                "Connected to Godot DAP".green().bold(),
                "(via daemon)".dimmed(),
            );
            println!();
            if let Some(caps) = result.get("capabilities").and_then(|c| c.as_object()) {
                println!("{}", "Capabilities:".bold());
                for (k, v) in caps {
                    if v.as_bool() == Some(true) {
                        println!("  {} {}", "+".green(), k);
                    }
                }
            }
            if let Some(threads) = result.get("threads").and_then(|t| t.as_array()) {
                println!();
                println!("{}", "Threads:".bold());
                for t in threads {
                    println!(
                        "  {} {} (id: {})",
                        "*".cyan(),
                        t["name"].as_str().unwrap_or("?"),
                        t["id"].as_i64().unwrap_or(0)
                    );
                }
            }
        }
    }

    Ok(())
}

// ── One-shot: stop ──────────────────────────────────────────────────

fn cmd_stop() -> Result<()> {
    // Continue execution first in case paused at a breakpoint
    daemon_dap("dap_continue", serde_json::json!({}));
    daemon_dap("dap_terminate", serde_json::json!({}))
        .ok_or_else(|| miette!("Could not terminate game — is a game running?"))?;
    println!("{} Game terminated", "■".red());
    Ok(())
}

// ── One-shot: continue/next/step/pause/eval ─────────────────────────

fn cmd_continue(args: StepArgs) -> Result<()> {
    daemon_dap("dap_continue", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to continue — is a game running and paused?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Continued".green()),
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"action": "continue"})).unwrap()
            );
        }
    }
    Ok(())
}

fn cmd_next(args: StepArgs) -> Result<()> {
    daemon_dap("dap_next", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Stepped over".green()),
        OutputFormat::Json => print_step_json("next"),
    }
    Ok(())
}

fn cmd_step(args: StepArgs) -> Result<()> {
    daemon_dap("dap_step_in", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Stepped in".green()),
        OutputFormat::Json => print_step_json("step"),
    }
    Ok(())
}

fn cmd_step_out(args: StepArgs) -> Result<()> {
    // Synthetic step-out: repeat `next` until stack depth decreases.
    // Godot's DAP doesn't support stepOut natively (the VS Code plugin
    // uses the same approach via the binary debug protocol).
    let initial_depth = get_stack_frames().len();
    if initial_depth <= 1 {
        return Err(miette!("Cannot step out — already at the top-level frame."));
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);

    loop {
        daemon_dap("dap_next", serde_json::json!({}))
            .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;

        // Wait for Godot to stop after the step
        let stopped = daemon_dap_timeout("dap_wait_stopped", serde_json::json!({"timeout": 5}), 5);
        if stopped.is_none() {
            return Err(miette!("Step-out timed out waiting for execution to stop."));
        }

        std::thread::sleep(std::time::Duration::from_millis(50));

        let new_depth = get_stack_frames().len();
        if new_depth < initial_depth {
            break; // Successfully stepped out
        }

        if std::time::Instant::now() >= deadline {
            return Err(miette!(
                "Step-out timed out after 15s — function may have a long-running loop.\n  \
                 Use `gd debug continue` to resume, or set a breakpoint in the caller instead."
            ));
        }
    }

    match args.format {
        OutputFormat::Human => println!("{}", "Stepped out".green()),
        OutputFormat::Json => print_step_json("step-out"),
    }
    Ok(())
}

/// Wait for stopped event after a step, print JSON with stack frames + variables.
fn print_step_json(action: &str) {
    let stopped = daemon_dap_timeout("dap_wait_stopped", serde_json::json!({"timeout": 3}), 3);
    if stopped.is_some() {
        // Brief pause for Godot to populate scope data
        std::thread::sleep(std::time::Duration::from_millis(100));
        let frames = get_stack_frames();
        let vars = collect_frame_variables(frames.first().map(|f| f.id));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": action, "stopped": true, "stackFrames": frames, "variables": vars,
            }))
            .unwrap()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": action, "stopped": false,
            }))
            .unwrap()
        );
    }
}

/// Collect variables from all scopes for a given frame.
fn collect_frame_variables(frame_id: Option<i64>) -> Vec<serde_json::Value> {
    let Some(fid) = frame_id else {
        return vec![];
    };
    let Some(scopes_body) = daemon_dap("dap_scopes", serde_json::json!({"frame_id": fid})) else {
        return vec![];
    };
    let Some(scopes) = scopes_body["scopes"].as_array() else {
        return vec![];
    };
    let mut result = Vec::new();
    for scope in scopes {
        let name = scope["name"].as_str().unwrap_or("?");
        let vref = scope["variablesReference"].as_i64().unwrap_or(0);
        if vref > 0
            && let Some(vbody) = daemon_dap(
                "dap_variables",
                serde_json::json!({"variables_reference": vref}),
            )
        {
            result.push(serde_json::json!({
                "scope": name,
                "variables": parse_variables(&vbody),
            }));
        }
    }
    result
}

fn cmd_pause(args: StepArgs) -> Result<()> {
    daemon_dap("dap_pause", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to pause — is a game running?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Paused".green()),
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"action": "pause"})).unwrap()
            );
        }
    }
    Ok(())
}

fn cmd_eval(args: EvalArgs) -> Result<()> {
    let expr = args.expr.trim();
    if expr.is_empty() {
        return Err(eval_error(&args, "--expr cannot be empty"));
    }

    // Warn on assignment syntax — Godot's eval doesn't persist direct assignments
    if is_likely_assignment(expr) {
        if matches!(args.format, OutputFormat::Json) {
            // In JSON mode, include warning in the output later
        } else {
            eprintln!(
                "{} Direct assignment via eval may return <null> and not persist.",
                "Warning:".yellow().bold(),
            );
            if let Some(lhs) = extract_assignment_lhs(expr) {
                eprintln!("  Use: gd debug eval --expr \"self.set('{}', ...)\"", lhs);
            }
        }
    }

    let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
    let result = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": expr, "context": "repl", "frame_id": frame_id}),
    )
    .ok_or_else(|| {
        eval_error(
            &args,
            "Evaluate failed — game must be paused at a breakpoint.\n  Use `gd debug break` to pause (not `gd debug pause`, which lacks stack frame context).",
        )
    })?;

    let value = result["result"].as_str().unwrap_or("?");
    let type_name = result["type"].as_str().unwrap_or("");

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if type_name.is_empty() {
                println!("{} = {}", expr.cyan(), value.green());
            } else {
                println!("{} {} = {}", type_name.dimmed(), expr.cyan(), value.green());
            }
            // Hint on <null> results — but not for method calls (void return is expected)
            if (value == "<null>" || value == "Null") && !expr.contains('(') {
                eprintln!(
                    "  {}",
                    "Hint: <null> may indicate an undefined variable or unsupported expression"
                        .dimmed()
                );
            }
        }
    }
    Ok(())
}

/// Detect direct assignment syntax (= but not ==, !=, <=, >=, :=, +=, etc.)
fn is_likely_assignment(expr: &str) -> bool {
    // Skip set() calls — those are intentional
    if expr.contains(".set(") || expr.contains(".set_") {
        return false;
    }
    let bytes = expr.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        // Skip ==
        if next == b'=' {
            continue;
        }
        // Skip !=, <=, >=, :=, +=, -=, *=, /=, == (second =)
        if matches!(
            prev,
            b'!' | b'<' | b'>' | b':' | b'+' | b'-' | b'*' | b'/' | b'='
        ) {
            continue;
        }
        return true;
    }
    false
}

/// Extract the left-hand side of an assignment (e.g. "self.speed" from "self.speed = 5")
fn extract_assignment_lhs(expr: &str) -> Option<&str> {
    let eq_pos = expr.find('=')?;
    let lhs = expr[..eq_pos].trim();
    let prop = lhs.strip_prefix("self.").unwrap_or(lhs);
    if prop.is_empty() {
        return None;
    }
    Some(prop)
}

fn cmd_set_var(args: SetVarArgs) -> Result<()> {
    let frames = get_stack_frames();
    let frame = frames
        .first()
        .ok_or_else(|| set_var_error(&args, "No stack frames — game must be paused at a breakpoint.\n  Use `gd debug break` to pause at a breakpoint first."))?;

    // Use eval with self.set() — fast path (Godot's DAP setVariable is broken)
    let val_literal = gdscript_value_literal(&args.value);
    let set_expr = format!("self.set(\"{}\", {val_literal})", args.name);
    daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": set_expr, "context": "repl", "frame_id": frame.id}),
    )
    .ok_or_else(|| {
        set_var_error(
            &args,
            &format!(
                "Failed to set '{}' — game must be paused at a breakpoint.",
                args.name
            ),
        )
    })?;

    // Verify by reading back
    let verify_expr = format!("self.{}", args.name);
    let verify_result = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": verify_expr, "context": "repl", "frame_id": frame.id}),
    );
    let new_val = verify_result
        .as_ref()
        .and_then(|v| v["result"].as_str())
        .unwrap_or("<null>");

    // If verification shows <null>, the property might not exist or it's a local
    if new_val == "<null>" || new_val == "Null" {
        // Check if it's a local variable (more specific error)
        if is_local_variable(frame.id, &args.name) {
            return Err(set_var_error(
                &args,
                &format!(
                    "Cannot modify local variable '{}' — Godot's DAP does not support setting locals.\n  \
                     Only member variables can be modified via `set-var` or `eval --expr \"self.set('name', value)\"`.",
                    args.name,
                ),
            ));
        }
        return Err(set_var_error(
            &args,
            &format!(
                "Failed to set '{}' — variable not found as a member property on self.\n  \
                 Only member variables (declared with `var` at class level) can be set.",
                args.name,
            ),
        ));
    }

    // Get type from verify result; fall back to inferring from value.
    // If we auto-quoted the input, we know it's a String (Godot's eval returns
    // string values without quotes, so infer_gdscript_type can't detect them).
    let was_auto_quoted = val_literal != args.value && val_literal.starts_with('"');
    let type_name = verify_result
        .as_ref()
        .and_then(|v| v["type"].as_str())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            if was_auto_quoted {
                "String"
            } else {
                infer_gdscript_type(new_val)
            }
        });

    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": args.name,
                    "value": new_val,
                    "type": type_name,
                    "input": args.value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            print_set_result(type_name, &args.name, new_val);
        }
    }
    Ok(())
}

/// Build a set-var error that outputs JSON when --format json is active.
fn set_var_error(args: &SetVarArgs, message: &str) -> miette::Report {
    if matches!(args.format, OutputFormat::Json) {
        // Print JSON error and exit with non-zero (miette will set exit code)
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "error": message,
                "name": args.name,
                "input": args.value,
            }))
            .unwrap()
        );
    }
    miette!("{}", message)
}

/// Infer a GDScript type name from a value string.
fn infer_gdscript_type(value: &str) -> &str {
    if value == "true" || value == "false" || value == "True" || value == "False" {
        return "bool";
    }
    if value.parse::<i64>().is_ok() {
        return "int";
    }
    if value.parse::<f64>().is_ok() {
        return "float";
    }
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return "String";
    }
    // Constructor types: Vector2(...), Color(...), etc.
    if let Some(paren) = value.find('(') {
        return &value[..paren];
    }
    ""
}

/// Build an eval error that outputs JSON when --format json is active.
fn eval_error(args: &EvalArgs, message: &str) -> miette::Report {
    if matches!(args.format, OutputFormat::Json) {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "error": message,
                "expression": args.expr,
            }))
            .unwrap()
        );
    }
    miette!("{}", message)
}

/// Check if a variable name exists in the Locals scope.
fn is_local_variable(frame_id: i64, name: &str) -> bool {
    let Some(scopes_body) = daemon_dap("dap_scopes", serde_json::json!({"frame_id": frame_id}))
    else {
        return false;
    };
    let Some(scopes) = scopes_body["scopes"].as_array() else {
        return false;
    };
    for scope in scopes {
        let scope_name = scope["name"].as_str().unwrap_or("");
        if !scope_name.to_lowercase().contains("local") {
            continue;
        }
        let vref = scope["variablesReference"].as_i64().unwrap_or(0);
        if vref <= 0 {
            continue;
        }
        if let Some(vbody) = daemon_dap(
            "dap_variables",
            serde_json::json!({"variables_reference": vref}),
        ) {
            return vbody["variables"]
                .as_array()
                .is_some_and(|vars| vars.iter().any(|v| v["name"].as_str() == Some(name)));
        }
    }
    false
}

/// Convert a CLI value string to a GDScript literal expression.
/// Bare words like `bike` become `"bike"` (quoted strings).
/// Numbers, bools, constructors, and already-quoted strings pass through.
fn gdscript_value_literal(value: &str) -> String {
    // Already quoted
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return value.to_string();
    }
    // Number (int or float, including negatives)
    if value.parse::<f64>().is_ok() {
        return value.to_string();
    }
    // Boolean, null
    if matches!(value, "true" | "false" | "null") {
        return value.to_string();
    }
    // Constructor or expression: Vector3(1,2,3), Color.RED, Array(), etc.
    if value.contains('(') || value.contains('.') {
        return value.to_string();
    }
    // Bare word — treat as string literal
    format!("\"{value}\"")
}

fn print_set_result(type_name: &str, name: &str, value: &str) {
    if type_name.is_empty() {
        println!("{} {} = {}", "Set".green(), name.cyan(), value.green());
    } else {
        println!(
            "{} {} {} = {}",
            "Set".green(),
            type_name.dimmed(),
            name.cyan(),
            value.green()
        );
    }
}

// ── One-shot: scene-tree ─────────────────────────────────────────────

fn cmd_scene_tree(args: SceneTreeArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap("debug_scene_tree", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to get scene tree"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", "Scene tree:".bold());
            if let Some(nodes) = result.get("nodes").and_then(|n| n.as_array()) {
                for node in nodes {
                    print_scene_node(node, 1);
                }
            } else if let Some(nodes) = result.as_array() {
                for node in nodes {
                    print_scene_node(node, 1);
                }
            } else {
                print_scene_node(&result, 1);
            }
        }
    }
    Ok(())
}

fn print_scene_node(node: &serde_json::Value, indent: usize) {
    let name = node["name"].as_str().unwrap_or("?");
    let class = node["class_name"].as_str().unwrap_or("");
    let id = node["object_id"].as_u64().unwrap_or(0);
    let scene = node["scene_file_path"].as_str().unwrap_or("");
    let pad = "  ".repeat(indent);
    let scene_info = if scene.is_empty() {
        String::new()
    } else {
        format!(" {}", scene.dimmed())
    };
    if class.is_empty() {
        println!("{pad}{name} {}{scene_info}", format!("[id: {id}]").dimmed());
    } else {
        println!(
            "{pad}{} {} {}{scene_info}",
            name.cyan(),
            format!("({class})").dimmed(),
            format!("[id: {id}]").dimmed(),
        );
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            print_scene_node(child, indent + 1);
        }
    }
}

// ── Variant display helper ───────────────────────────────────────────

/// Format a serialized GodotVariant JSON value for human display.
/// GodotVariant serializes as `{"type": "Int", "value": 42}`.
fn format_variant_display(v: &serde_json::Value) -> String {
    let Some(typ) = v.get("type").and_then(|t| t.as_str()) else {
        return if let Some(s) = v.as_str() {
            s.to_string()
        } else {
            v.to_string()
        };
    };
    let val = v.get("value");
    match typ {
        "Nil" => "null".to_string(),
        "Bool" | "Int" | "Float" => val.map(|v| v.to_string()).unwrap_or_default(),
        "String" | "StringName" | "NodePath" => {
            val.and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
        "Vector2" | "Vector3" | "Vector4" | "Vector2i" | "Vector3i" | "Vector4i" | "Color"
        | "Rect2" | "Rect2i" | "Transform2D" | "Basis" | "Transform3D" | "Quaternion" | "AABB"
        | "Plane" | "Projection" => {
            if let Some(arr) = val.and_then(|v| v.as_array()) {
                let parts: Vec<String> = arr.iter().map(|c| c.to_string()).collect();
                format!("{typ}({})", parts.join(", "))
            } else {
                val.map(|v| v.to_string()).unwrap_or_default()
            }
        }
        "ObjectId" => val.map(|v| format!("Object#{v}")).unwrap_or_default(),
        _ => val
            .map(|v| v.to_string())
            .unwrap_or_else(|| typ.to_string()),
    }
}

// ── One-shot: inspect ───────────────────────────────────────────────

fn cmd_inspect(args: InspectArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap("debug_inspect", serde_json::json!({"object_id": args.id}))
        .ok_or_else(|| {
            miette!(
                "Failed to inspect object {} — is a game running with the binary debug protocol?",
                args.id
            )
        })?;

    if args.brief {
        return print_inspect_brief(&result, args.id, &args.format);
    }

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            let class = result["class_name"].as_str().unwrap_or("Object");
            println!(
                "{} {}",
                class.cyan().bold(),
                format!("(id: {})", args.id).dimmed(),
            );
            println!("{}", "Properties:".bold());
            if let Some(props) = result["properties"].as_array() {
                if props.is_empty() {
                    println!("  {}", "(none)".dimmed());
                }
                for p in props {
                    let pname = p["name"].as_str().unwrap_or("?");
                    let pval = format_variant_display(&p["value"]);
                    println!("  {} = {}", pname.cyan(), pval.green());
                }
            } else {
                println!("  {}", "(no properties returned)".dimmed());
            }
        }
    }
    Ok(())
}

/// Properties to hide in --brief mode (Godot internals, not useful for debugging).
/// Uses usage flags: bit 1 (PROPERTY_USAGE_EDITOR) = 2, bit 13 (PROPERTY_USAGE_INTERNAL) = 8192
const BRIEF_HIDDEN_PROPS: &[&str] = &[
    "script",
    "owner",
    "multiplayer",
    "process_mode",
    "process_priority",
    "process_physics_priority",
    "process_thread_group",
    "process_thread_group_order",
    "process_thread_messages",
    "physics_interpolation_mode",
    "auto_translate_mode",
    "editor_description",
    "unique_name_in_owner",
];

/// Print inspect output in brief mode: just {name: value} pairs, no Godot internals.
fn print_inspect_brief(result: &serde_json::Value, id: u64, format: &OutputFormat) -> Result<()> {
    let props = result["properties"].as_array();
    match format {
        OutputFormat::Json => {
            let mut brief = serde_json::Map::new();
            brief.insert(
                "object_id".to_string(),
                serde_json::Value::Number(id.into()),
            );
            brief.insert("class_name".to_string(), result["class_name"].clone());
            let mut members = serde_json::Map::new();
            if let Some(props) = props {
                for p in props {
                    let name = p["name"].as_str().unwrap_or("?");
                    if BRIEF_HIDDEN_PROPS.contains(&name) {
                        continue;
                    }
                    members.insert(name.to_string(), p["value"].clone());
                }
            }
            brief.insert("properties".to_string(), serde_json::Value::Object(members));
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::Value::Object(brief)).unwrap()
            );
        }
        OutputFormat::Human => {
            let class = result["class_name"].as_str().unwrap_or("Object");
            println!("{} {}", class.cyan().bold(), format!("(id: {id})").dimmed(),);
            if let Some(props) = props {
                for p in props {
                    let pname = p["name"].as_str().unwrap_or("?");
                    if BRIEF_HIDDEN_PROPS.contains(&pname) {
                        continue;
                    }
                    let pval = format_variant_display(&p["value"]);
                    println!("  {} = {}", pname.cyan(), pval.green());
                }
            }
        }
    }
    Ok(())
}

// ── One-shot: set-prop ──────────────────────────────────────────────

fn cmd_set_prop(args: SetPropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    let result = daemon_dap(
        "debug_set_property",
        serde_json::json!({
            "object_id": args.id,
            "property": args.property,
            "value": json_value,
        }),
    )
    .ok_or_else(|| {
        miette!(
            "Failed to set property '{}' on object {} — is a game running with the binary debug protocol?",
            args.property,
            args.id
        )
    })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
        }
    }
    Ok(())
}

// ── One-shot: suspend ───────────────────────────────────────────────

fn cmd_suspend(args: SuspendArgs) -> Result<()> {
    ensure_binary_debug()?;
    let suspend = !args.off;
    let result =
        daemon_dap("debug_suspend", serde_json::json!({"suspend": suspend})).ok_or_else(|| {
            miette!(
                "Failed to {} game — is a game running with the binary debug protocol?",
                if suspend { "suspend" } else { "resume" }
            )
        })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if suspend {
                println!("{}", "Game suspended".green());
            } else {
                println!("{}", "Game resumed".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: next-frame ────────────────────────────────────────────

fn cmd_next_frame(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap("debug_next_frame", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to advance frame — is the game suspended?"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", "Advanced one frame".green());
        }
    }
    Ok(())
}

// ── One-shot: time-scale ────────────────────────────────────────────

fn cmd_time_scale(args: TimeScaleArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap("debug_time_scale", serde_json::json!({"scale": args.scale}))
        .ok_or_else(|| {
            miette!("Failed to set time scale — is a game running with the binary debug protocol?")
        })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", format!("Time scale set to {}x", args.scale).green());
        }
    }
    Ok(())
}

// ── One-shot: reload-scripts ────────────────────────────────────────

fn cmd_reload_scripts(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap("debug_reload_scripts", serde_json::json!({})).ok_or_else(|| {
        miette!("Failed to reload scripts — is a game running with the binary debug protocol?")
    })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", "Scripts reloaded".green());
        }
    }
    Ok(())
}

// ── One-shot: reload-all-scripts ─────────────────────────────────────

fn cmd_reload_all_scripts(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_reload_all_scripts", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running with the binary debug protocol?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"reloaded": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "All scripts reloaded".green()),
    }
    Ok(())
}

// ── One-shot: skip-breakpoints ──────────────────────────────────────

fn cmd_skip_breakpoints(args: SkipBreakpointsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let skip = !args.off;
    daemon_dap(
        "debug_set_skip_breakpoints",
        serde_json::json!({"value": skip}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"skip": skip})).unwrap()
            );
        }
        OutputFormat::Human => {
            if skip {
                println!("{}", "Breakpoints skipped".green());
            } else {
                println!("{}", "Breakpoints re-enabled".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: ignore-errors ─────────────────────────────────────────

fn cmd_ignore_errors(args: IgnoreErrorsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let ignore = !args.off;
    daemon_dap(
        "debug_set_ignore_error_breaks",
        serde_json::json!({"value": ignore}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ignore": ignore})).unwrap()
            );
        }
        OutputFormat::Human => {
            if ignore {
                println!("{}", "Error breaks ignored".green());
            } else {
                println!("{}", "Error breaks re-enabled".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: mute-audio ────────────────────────────────────────────

fn cmd_mute_audio(args: MuteAudioArgs) -> Result<()> {
    ensure_binary_debug()?;
    let mute = !args.off;
    daemon_dap("debug_mute_audio", serde_json::json!({"mute": mute}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"muted": mute})).unwrap()
            );
        }
        OutputFormat::Human => {
            if mute {
                println!("{}", "Audio muted".green());
            } else {
                println!("{}", "Audio unmuted".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: override-camera ───────────────────────────────────────

fn cmd_override_camera(args: OverrideCameraArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enable = !args.off;
    daemon_dap(
        "debug_override_cameras",
        serde_json::json!({"enable": enable}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"override": enable})).unwrap()
            );
        }
        OutputFormat::Human => {
            if enable {
                println!("{}", "Camera override enabled".green());
            } else {
                println!("{}", "Camera override disabled".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: save-node ─────────────────────────────────────────────

fn cmd_save_node(args: SaveNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_save_node",
        serde_json::json!({"object_id": args.id, "path": args.path}),
    )
    .ok_or_else(|| miette!("Failed to save node {} — is a game running?", args.id))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "saved": true,
                    "object_id": args.id,
                    "path": args.path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} node {} to {}",
                "Saved".green(),
                format!("[{}]", args.id).dimmed(),
                args.path.cyan(),
            );
        }
    }
    Ok(())
}

// ── One-shot: set-prop-field ────────────────────────────────────────

fn cmd_set_prop_field(args: SetPropFieldArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_dap(
        "debug_set_property_field",
        serde_json::json!({
            "object_id": args.id,
            "property": args.property,
            "field": args.field,
            "value": json_value,
        }),
    )
    .ok_or_else(|| {
        miette!(
            "Failed to set {}.{} on object {} — is a game running?",
            args.property,
            args.field,
            args.id
        )
    })?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "object_id": args.id,
                    "property": args.property,
                    "field": args.field,
                    "value": json_value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.field.cyan(),
                args.value.green(),
            );
        }
    }
    Ok(())
}

// ── One-shot: profiler ──────────────────────────────────────────────

fn cmd_profiler(args: ProfilerArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enable = !args.off;
    daemon_dap(
        "debug_toggle_profiler",
        serde_json::json!({"profiler": args.name, "enable": enable}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "profiler": args.name,
                    "enabled": enable,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            if enable {
                println!("{} profiler {}", "Enabled".green(), args.name.cyan());
            } else {
                println!("{} profiler {}", "Disabled".green(), args.name.cyan());
            }
        }
    }
    Ok(())
}

// ── One-shot: live editing ──────────────────────────────────────────

fn cmd_live_set_root(args: LiveSetRootArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_set_root",
        serde_json::json!({"scene_path": args.path, "scene_file": args.file}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "root": args.path,
                    "file": args.file,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} live root to {} {}",
                "Set".green(),
                args.path.cyan(),
                format!("({})", args.file).dimmed(),
            );
        }
    }
    Ok(())
}

fn cmd_live_create_node(args: LiveCreateNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_create_node",
        serde_json::json!({
            "parent": args.parent,
            "class": args.class_name,
            "name": args.name,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "created": true,
                    "name": args.name,
                    "class": args.class_name,
                    "parent": args.parent,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} {}",
                "Created".green(),
                args.name.cyan(),
                format!("({})", args.class_name).dimmed(),
            );
        }
    }
    Ok(())
}

fn cmd_live_instantiate(args: LiveInstantiateArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_instantiate_node",
        serde_json::json!({
            "parent": args.parent,
            "scene": args.scene,
            "name": args.name,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "instantiated": true,
                    "name": args.name,
                    "scene": args.scene,
                    "parent": args.parent,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} {}",
                "Instantiated".green(),
                args.name.cyan(),
                format!("({})", args.scene).dimmed(),
            );
        }
    }
    Ok(())
}

fn cmd_live_remove_node(args: LiveRemoveNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_remove_node",
        serde_json::json!({"path": args.path}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "removed": true,
                    "path": args.path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{} {}", "Removed".green(), args.path.cyan());
        }
    }
    Ok(())
}

fn cmd_live_duplicate(args: LiveDuplicateArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_duplicate_node",
        serde_json::json!({"path": args.path, "new_name": args.name}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "duplicated": true,
                    "source": args.path,
                    "name": args.name,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} as {}",
                "Duplicated".green(),
                args.path.cyan(),
                args.name.cyan(),
            );
        }
    }
    Ok(())
}

fn cmd_live_reparent(args: LiveReparentArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_reparent_node",
        serde_json::json!({
            "path": args.path,
            "new_parent": args.new_parent,
            "new_name": args.name,
            "pos": args.pos,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "reparented": true,
                    "path": args.path,
                    "new_parent": args.new_parent,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} to {}",
                "Reparented".green(),
                args.path.cyan(),
                args.new_parent.cyan(),
            );
        }
    }
    Ok(())
}

fn cmd_live_node_prop(args: LiveNodePropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_dap(
        "debug_live_node_prop",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "value": json_value,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "value": json_value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
        }
    }
    Ok(())
}

fn cmd_live_node_call(args: LiveNodeCallArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_args: serde_json::Value =
        serde_json::from_str(&args.args).unwrap_or_else(|_| serde_json::json!([]));

    daemon_dap(
        "debug_live_node_call",
        serde_json::json!({
            "id": args.id,
            "method": args.method,
            "args": json_args,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "method": args.method,
                    "args": json_args,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{}({})",
                "Called".green(),
                format!("[{}]", args.id).dimmed(),
                args.method.cyan(),
                args.args.dimmed(),
            );
        }
    }
    Ok(())
}

// ── Execution control (binary protocol) ─────────────────────────────

fn cmd_exec_continue(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Send debugger continue (resumes from breakpoint)
    daemon_dap("debug_continue", serde_json::json!({}));
    // Also unsuspend the scene tree and re-enable input (in case the game
    // was paused via suspend rather than a debugger breakpoint)
    daemon_dap("debug_suspend", serde_json::json!({"suspend": false}));
    daemon_dap("debug_node_select_set_type", serde_json::json!({"type": 0}));
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Continued".green()),
    }
    Ok(())
}

fn cmd_exec_pause(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Use scene-level suspend (freezes game loop + disables input)
    // rather than debugger break (which halts script execution)
    daemon_dap("debug_suspend", serde_json::json!({"suspend": true}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Paused".green()),
    }
    Ok(())
}

fn cmd_exec_next(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_next_step", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Stepped over".green()),
    }
    Ok(())
}

fn cmd_exec_step_in(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_step_in", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Stepped in".green()),
    }
    Ok(())
}

fn cmd_exec_step_out(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_step_out", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Stepped out".green()),
    }
    Ok(())
}

// ── Debugging (binary protocol) ─────────────────────────────────────

fn cmd_breakpoint(args: BreakpointBinArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enabled = !args.off;

    // Resolve --name to path:line if provided
    let (path, line) = if let Some(ref func_name) = args.name {
        let (p, l) = resolve_function_to_location(func_name)?;
        // --path/--line override --name if both given
        let path = args.path.unwrap_or(p);
        let line = args.line.unwrap_or(l);
        (path, line)
    } else {
        let path = args
            .path
            .ok_or_else(|| miette!("--path is required (or use --name to resolve by function)"))?;
        let line = args
            .line
            .ok_or_else(|| miette!("--line is required (or use --name to resolve by function)"))?;
        (path, line)
    };

    let mut bp_params = serde_json::json!({"path": path, "line": line, "enabled": enabled});
    if let Some(ref condition) = args.condition {
        bp_params["condition"] = serde_json::Value::String(condition.clone());
    }
    daemon_dap("debug_breakpoint", bp_params)
        .ok_or_else(|| miette!("Failed — is a game running?"))?;

    match args.format {
        OutputFormat::Json => {
            let mut out = serde_json::json!({
                "path": path,
                "line": line,
                "enabled": enabled,
            });
            if let Some(ref condition) = args.condition {
                out["condition"] = serde_json::Value::String(condition.clone());
            }
            if let Some(ref name) = args.name {
                out["name"] = serde_json::Value::String(name.clone());
            }
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        OutputFormat::Human => {
            let cond_info = args
                .condition
                .as_ref()
                .map(|c| format!(" when {c}"))
                .unwrap_or_default();
            if enabled {
                println!(
                    "{} at {}:{}{}",
                    "Breakpoint set".green(),
                    path.cyan(),
                    line,
                    cond_info.dimmed(),
                );
            } else {
                println!(
                    "{} at {}:{}",
                    "Breakpoint cleared".green(),
                    path.cyan(),
                    line,
                );
            }
        }
    }
    Ok(())
}

/// Resolve a function name to a res:// path and line number by searching project GDScript files.
fn resolve_function_to_location(func_name: &str) -> Result<(String, u32)> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let files = crate::core::fs::collect_gdscript_files(&project.root)?;

    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        // Search for "func <name>" pattern
        for (i, line_text) in source.lines().enumerate() {
            let trimmed = line_text.trim();
            if trimmed.starts_with("func ")
                && trimmed[5..].trim_start().starts_with(func_name)
                && trimmed[5..]
                    .trim_start()
                    .get(func_name.len()..)
                    .is_some_and(|rest| {
                        rest.starts_with('(')
                            || rest.starts_with(':')
                            || rest.starts_with(' ')
                            || rest.is_empty()
                    })
            {
                // Convert to res:// path
                let rel = file
                    .strip_prefix(&project.root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let res_path = format!("res://{rel}");
                return Ok((res_path, (i + 1) as u32));
            }
        }
    }

    Err(miette!(
        "Function '{}' not found in any .gd file in the project",
        func_name
    ))
}

fn cmd_stack(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap("debug_get_stack_dump", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if let Some(frames) = result.as_array() {
                if frames.is_empty() {
                    println!("{}", "(no stack frames)".dimmed());
                }
                for (i, f) in frames.iter().enumerate() {
                    let name = f["function"]
                        .as_str()
                        .or_else(|| f["name"].as_str())
                        .unwrap_or("?");
                    let file = f["file"].as_str().unwrap_or("?");
                    let line = f["line"].as_u64().unwrap_or(0);
                    println!(
                        "  {} {} ({}:{})",
                        format!("#{i}").dimmed(),
                        name.green().bold(),
                        file.cyan(),
                        line,
                    );
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
    }
    Ok(())
}

fn cmd_vars(args: VarsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap(
        "debug_get_stack_frame_vars",
        serde_json::json!({"frame": args.frame}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if let Some(vars) = result.as_array() {
                if vars.is_empty() {
                    println!("{}", "(no variables)".dimmed());
                }
                for v in vars {
                    let name = v["name"].as_str().unwrap_or("?");
                    let value = format_variant_display(&v["value"]);
                    println!("  {} = {}", name.cyan(), value.green());
                }
            } else if let Some(obj) = result.as_object() {
                // Daemon may return named scope groups
                for (scope_name, scope_vars) in obj {
                    println!("\n{}", format!("{scope_name}:").bold());
                    if let Some(vars) = scope_vars.as_array() {
                        for v in vars {
                            let name = v["name"].as_str().unwrap_or("?");
                            let value = format_variant_display(&v["value"]);
                            println!("  {} = {}", name.cyan(), value.green());
                        }
                    }
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
    }
    Ok(())
}

fn cmd_evaluate(args: EvalBinArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap(
        "debug_evaluate",
        serde_json::json!({"expression": args.expr, "frame": args.frame}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            let display = format_variant_display(&result);
            println!("{}", display.green());
        }
    }
    Ok(())
}

// ── Multi-object inspection (binary protocol) ───────────────────────

fn cmd_inspect_objects(args: InspectObjectsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_dap(
        "debug_inspect_objects",
        serde_json::json!({"ids": args.id, "selection": args.selection}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            let objects = result.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            for obj in objects {
                let class = obj["class_name"].as_str().unwrap_or("Object");
                let oid = obj["object_id"].as_u64().unwrap_or(0);
                println!(
                    "{} {}",
                    class.cyan().bold(),
                    format!("(id: {oid})").dimmed(),
                );
                println!("{}", "Properties:".bold());
                if let Some(props) = obj["properties"].as_array() {
                    if props.is_empty() {
                        println!("  {}", "(none)".dimmed());
                    }
                    for p in props {
                        let pname = p["name"].as_str().unwrap_or("?");
                        let pval = format_variant_display(&p["value"]);
                        println!("  {} = {}", pname.cyan(), pval.green());
                    }
                } else {
                    println!("  {}", "(no properties returned)".dimmed());
                }
                println!();
            }
            if objects.is_empty() {
                println!("{}", "(no objects returned)".dimmed());
            }
        }
    }
    Ok(())
}

// ── Camera (binary protocol) ────────────────────────────────────────

fn cmd_transform_camera_2d(args: TransformCamera2dArgs) -> Result<()> {
    ensure_binary_debug()?;
    let parsed: serde_json::Value = serde_json::from_str(&args.transform)
        .map_err(|e| miette!("Invalid transform JSON: {e}"))?;
    if let Some(arr) = parsed.as_array() {
        if arr.len() != 6 {
            return Err(miette!(
                "2D transform requires exactly 6 floats, got {}",
                arr.len()
            ));
        }
    } else {
        return Err(miette!("Transform must be a JSON array of 6 floats"));
    }
    daemon_dap(
        "debug_transform_camera_2d",
        serde_json::json!({"transform": parsed}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "2D camera transformed".green()),
    }
    Ok(())
}

fn cmd_transform_camera_3d(args: TransformCamera3dArgs) -> Result<()> {
    ensure_binary_debug()?;
    let parsed: serde_json::Value = serde_json::from_str(&args.transform)
        .map_err(|e| miette!("Invalid transform JSON: {e}"))?;
    if let Some(arr) = parsed.as_array() {
        if arr.len() != 12 {
            return Err(miette!(
                "3D transform requires exactly 12 floats, got {}",
                arr.len()
            ));
        }
    } else {
        return Err(miette!("Transform must be a JSON array of 12 floats"));
    }
    daemon_dap(
        "debug_transform_camera_3d",
        serde_json::json!({
            "transform": parsed,
            "perspective": args.perspective,
            "fov": args.fov,
            "near": args.near,
            "far": args.far,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "3D camera transformed".green()),
    }
    Ok(())
}

// ── Screenshot (binary protocol) ────────────────────────────────────

fn cmd_screenshot(args: ScreenshotArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Use a monotonic counter for the screenshot ID
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let result = daemon_dap("debug_request_screenshot", serde_json::json!({"id": id}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;

    let width = result["width"].as_u64().unwrap_or(0);
    let height = result["height"].as_u64().unwrap_or(0);
    let b64_data = result["data"]
        .as_str()
        .ok_or_else(|| miette!("No screenshot data in response"))?;

    // Decode base64 PNG and write to output file
    use base64::Engine;
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64_data)
        .map_err(|e| miette!("Failed to decode screenshot data: {e}"))?;

    std::fs::write(&args.output, &png_bytes)
        .map_err(|e| miette!("Failed to write screenshot to {}: {e}", args.output))?;

    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "path": args.output,
                    "width": width,
                    "height": height,
                    "size": png_bytes.len(),
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            let size_kb = png_bytes.len() / 1024;
            println!(
                "{} {}x{} ({size_kb} KB) → {}",
                "Screenshot saved".green(),
                width,
                height,
                args.output.cyan(),
            );
        }
    }
    Ok(())
}

// ── File management (binary protocol) ───────────────────────────────

fn cmd_reload_cached(args: ReloadCachedArgs) -> Result<()> {
    ensure_binary_debug()?;
    let count = args.file.len();
    daemon_dap(
        "debug_reload_cached_files",
        serde_json::json!({"files": args.file}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "count": count}))
                    .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{}", format!("Reloaded {count} cached files").green());
        }
    }
    Ok(())
}

// ── Node selection (binary protocol) ────────────────────────────────

fn cmd_node_select_type(args: NodeSelectIntArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_node_select_set_type",
        serde_json::json!({"type": args.value}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "type": args.value}))
                    .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{}",
                format!("Node select type set to {}", args.value).green()
            );
        }
    }
    Ok(())
}

fn cmd_node_select_mode(args: NodeSelectIntArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_node_select_set_mode",
        serde_json::json!({"mode": args.value}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "mode": args.value}))
                    .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{}",
                format!("Node select mode set to {}", args.value).green()
            );
        }
    }
    Ok(())
}

fn cmd_node_select_visible(args: ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let visible = !args.off;
    daemon_dap(
        "debug_node_select_set_visible",
        serde_json::json!({"visible": visible}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "visible": visible}))
                    .unwrap()
            );
        }
        OutputFormat::Human => {
            if visible {
                println!("{}", "Node visibility filter enabled".green());
            } else {
                println!("{}", "Node visibility filter disabled".green());
            }
        }
    }
    Ok(())
}

fn cmd_node_select_avoid_locked(args: ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let avoid = !args.off;
    daemon_dap(
        "debug_node_select_set_avoid_locked",
        serde_json::json!({"avoid": avoid}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "avoid": avoid}))
                    .unwrap()
            );
        }
        OutputFormat::Human => {
            if avoid {
                println!("{}", "Avoid locked nodes enabled".green());
            } else {
                println!("{}", "Avoid locked nodes disabled".green());
            }
        }
    }
    Ok(())
}

fn cmd_node_select_prefer_group(args: ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let prefer = !args.off;
    daemon_dap(
        "debug_node_select_set_prefer_group",
        serde_json::json!({"prefer": prefer}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "prefer": prefer}))
                    .unwrap()
            );
        }
        OutputFormat::Human => {
            if prefer {
                println!("{}", "Prefer group enabled".green());
            } else {
                println!("{}", "Prefer group disabled".green());
            }
        }
    }
    Ok(())
}

fn cmd_node_select_reset_cam_2d(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_node_select_reset_camera_2d", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "2D selection camera reset".green()),
    }
    Ok(())
}

fn cmd_node_select_reset_cam_3d(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_node_select_reset_camera_3d", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "3D selection camera reset".green()),
    }
    Ok(())
}

fn cmd_clear_selection(args: StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap("debug_clear_selection", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Selection cleared".green()),
    }
    Ok(())
}

// ── Live editing: resource operations (binary protocol) ─────────────

fn cmd_live_node_path(args: LivePathArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_node_path",
        serde_json::json!({"path": args.path, "id": args.id}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": args.path,
                    "id": args.id,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} for {}",
                "Live node path set".green(),
                format!("[{}]", args.id).dimmed(),
            );
        }
    }
    Ok(())
}

fn cmd_live_res_path(args: LivePathArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_res_path",
        serde_json::json!({"path": args.path, "id": args.id}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": args.path,
                    "id": args.id,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} for {}",
                "Live resource path set".green(),
                format!("[{}]", args.id).dimmed(),
            );
        }
    }
    Ok(())
}

fn cmd_live_res_prop(args: LiveNodePropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_dap(
        "debug_live_res_prop",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "value": json_value,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "value": json_value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
        }
    }
    Ok(())
}

fn cmd_live_node_prop_res(args: LivePropResArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_node_prop_res",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "res_path": args.res_path,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "res_path": args.res_path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.res_path.cyan(),
            );
        }
    }
    Ok(())
}

fn cmd_live_res_prop_res(args: LivePropResArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_res_prop_res",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "res_path": args.res_path,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "res_path": args.res_path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.res_path.cyan(),
            );
        }
    }
    Ok(())
}

fn cmd_live_res_call(args: LiveNodeCallArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_args: serde_json::Value =
        serde_json::from_str(&args.args).unwrap_or_else(|_| serde_json::json!([]));

    daemon_dap(
        "debug_live_res_call",
        serde_json::json!({
            "id": args.id,
            "method": args.method,
            "args": json_args,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "method": args.method,
                    "args": json_args,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{}({})",
                "Called".green(),
                format!("[{}]", args.id).dimmed(),
                args.method.cyan(),
                args.args.dimmed(),
            );
        }
    }
    Ok(())
}

// ── Live editing: advanced node operations (binary protocol) ────────

fn cmd_live_remove_keep(args: LiveRemoveKeepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_remove_and_keep_node",
        serde_json::json!({"path": args.path, "object_id": args.object_id}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": args.path,
                    "object_id": args.object_id,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{} {}", "Removed (kept)".green(), args.path.cyan(),);
        }
    }
    Ok(())
}

fn cmd_live_restore(args: LiveRestoreArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_dap(
        "debug_live_restore_node",
        serde_json::json!({
            "object_id": args.object_id,
            "path": args.path,
            "pos": args.pos,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "object_id": args.object_id,
                    "path": args.path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{} at {}", "Restored node".green(), args.path.cyan(),);
        }
    }
    Ok(())
}

// ── Helper: resolve function name to file:line ──────────────────────

/// Resolve a function name to (file, first_statement_line) by searching project symbols.
///
/// If `file_filter` is provided, only search that file. Otherwise search all
/// project files and error with a candidate list when the name is ambiguous.
/// Returns the first executable statement line inside the function body
/// (not the `func` declaration line, which Godot won't break on).
fn resolve_function_name(name: &str, file_filter: Option<&str>) -> Result<(String, u32)> {
    let cwd = std::env::current_dir().map_err(|e| miette!("cannot get current directory: {e}"))?;
    let project_root = crate::core::config::find_project_root(&cwd)
        .ok_or_else(|| miette!("no project.godot found"))?;

    let files = crate::core::fs::collect_gdscript_files(&project_root)
        .map_err(|e| miette!("failed to collect GDScript files: {e}"))?;

    let mut candidates: Vec<(String, u32)> = Vec::new();

    for file_path in &files {
        let rel = crate::core::fs::relative_slash(file_path, &project_root);
        if let Some(filter) = file_filter
            && rel != filter
        {
            continue;
        }
        if let Ok(symbols) = crate::lsp::query::query_symbols(&rel) {
            for sym in &symbols {
                if sym.name == name && sym.kind == "function" {
                    // Find the first statement line inside the function body
                    let body_line = find_first_body_line(file_path, sym.line).unwrap_or(sym.line);
                    candidates.push((rel.clone(), body_line));
                }
            }
        }
    }

    match candidates.len() {
        0 => {
            if let Some(filter) = file_filter {
                Err(miette!("function '{}' not found in '{}'", name, filter,))
            } else {
                Err(miette!("function '{}' not found in project", name))
            }
        }
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => {
            if file_filter.is_some() {
                // Multiple overloads in same file — just use the first
                Ok(candidates.into_iter().next().unwrap())
            } else {
                let list = candidates
                    .iter()
                    .map(|(f, l)| format!("  {}:{}", f, l))
                    .collect::<Vec<_>>()
                    .join("\n");
                Err(miette!(
                    "function '{}' is ambiguous — found in {} files:\n{}\n\n\
                     Use --file to disambiguate, e.g.:\n  \
                     gd debug break --name {} --file {}",
                    name,
                    candidates.len(),
                    list,
                    name,
                    candidates[0].0,
                ))
            }
        }
    }
}

/// Find the line number of the first executable statement inside a function body.
/// `func_line` is 1-based (the `func` declaration line from symbols).
/// Returns the 1-based line of the first non-comment, non-empty statement in the body.
fn find_first_body_line(file_path: &std::path::Path, func_line: u32) -> Option<u32> {
    let source = std::fs::read_to_string(file_path).ok()?;
    let tree = crate::core::parser::parse(&source).ok()?;
    let root = tree.root_node();

    // Find the function_definition or constructor_definition at this line
    let target_row = func_line - 1; // tree-sitter is 0-based
    let func_node = find_function_at_line(root, target_row)?;

    // Get the body node
    let body = func_node.child_by_field_name("body")?;

    // Find the first non-comment child of the body
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.is_named() && child.kind() != "comment" {
            return Some(child.start_position().row as u32 + 1); // 1-based
        }
    }
    None
}

/// Recursively find a function_definition or constructor_definition node at the given row.
fn find_function_at_line(node: tree_sitter::Node, target_row: u32) -> Option<tree_sitter::Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "function_definition" | "constructor_definition"
        ) && child.start_position().row as u32 == target_row
        {
            return Some(child);
        }
        // Recurse into class bodies
        if let Some(found) = find_function_at_line(child, target_row) {
            return Some(found);
        }
    }
    None
}

// ── Shared helpers ──────────────────────────────────────────────────

fn get_stack_frames() -> Vec<StackFrame> {
    let thread_id = daemon_dap("dap_threads", serde_json::json!({}))
        .and_then(|b| b["threads"].as_array()?.first()?.get("id")?.as_i64())
        .unwrap_or(1);

    daemon_dap(
        "dap_stack_trace",
        serde_json::json!({"thread_id": thread_id}),
    )
    .and_then(|b| {
        Some(
            b["stackFrames"]
                .as_array()?
                .iter()
                .map(|f| StackFrame {
                    id: f["id"].as_i64().unwrap_or(0),
                    name: f["name"].as_str().unwrap_or("?").to_string(),
                    file: f["source"]["name"].as_str().unwrap_or("?").to_string(),
                    line: f["line"].as_u64().unwrap_or(0) as u32,
                })
                .collect(),
        )
    })
    .unwrap_or_default()
}

fn parse_breakpoint_results(body: &serde_json::Value) -> Vec<BreakpointResult> {
    body["breakpoints"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|bp| BreakpointResult {
                    verified: bp["verified"].as_bool().unwrap_or(false),
                    line: bp["line"].as_u64().unwrap_or(0) as u32,
                    id: bp["id"].as_i64(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_variables(body: &serde_json::Value) -> Vec<Variable> {
    body["variables"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| Variable {
                    name: v["name"].as_str().unwrap_or("?").to_string(),
                    value: v["value"].as_str().unwrap_or("").to_string(),
                    type_name: v["type"].as_str().unwrap_or("").to_string(),
                    variables_reference: v["variablesReference"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── REPL helpers (all daemon-backed) ─────────────────────────────────

fn repl_stack() {
    let frames = get_stack_frames();
    if frames.is_empty() {
        println!(
            "{}",
            "No stack frames — game may not be paused at a breakpoint.".yellow()
        );
    } else {
        println!("{}", "Call stack:".bold());
        for (i, f) in frames.iter().enumerate() {
            println!(
                "  {} {} ({}:{})",
                format!("#{i}").dimmed(),
                f.name.green().bold(),
                f.file.cyan(),
                f.line
            );
        }
    }
}

fn repl_vars(scope_filter: Option<&str>) {
    let frames = get_stack_frames();
    let Some(frame) = frames.first() else {
        println!(
            "{}",
            "No stack frames — game may not be paused at a breakpoint.".yellow()
        );
        return;
    };

    let Some(scopes_body) = daemon_dap("dap_scopes", serde_json::json!({"frame_id": frame.id}))
    else {
        println!("{}", "Failed to get scopes.".red());
        return;
    };

    let scopes: Vec<Scope> = scopes_body["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|s| Scope {
                    name: s["name"].as_str().unwrap_or("?").to_string(),
                    variables_reference: s["variablesReference"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();

    let filter = scope_filter.map(|s| s.to_lowercase());
    for scope in &scopes {
        if let Some(ref f) = filter
            && !scope.name.to_lowercase().contains(f)
        {
            continue;
        }
        if scope.variables_reference > 0
            && let Some(body) = daemon_dap(
                "dap_variables",
                serde_json::json!({"variables_reference": scope.variables_reference}),
            )
        {
            let vars = parse_variables(&body);
            let _ = print_variables(&vars, &OutputFormat::Human, Some(&scope.name));
        }
    }
}

fn repl_expand(vref: i64) {
    if let Some(body) = daemon_dap(
        "dap_variables",
        serde_json::json!({"variables_reference": vref}),
    ) {
        let vars = parse_variables(&body);
        let _ = print_variables(&vars, &OutputFormat::Human, None);
    } else {
        println!("{}", "Failed to expand variable.".red());
    }
}

fn repl_eval(expr: &str) {
    // Get the top frame ID for evaluation context
    let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
    if let Some(body) = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": expr, "context": "repl", "frame_id": frame_id}),
    ) {
        let result = body["result"].as_str().unwrap_or("?");
        let type_name = body["type"].as_str().unwrap_or("");
        if type_name.is_empty() {
            println!("{} = {}", expr.cyan(), result.green());
        } else {
            println!(
                "{} {} = {}",
                type_name.dimmed(),
                expr.cyan(),
                result.green()
            );
        }
    } else {
        println!(
            "{}",
            "Evaluate failed or timed out. Godot only supports member-access expressions (e.g. self.speed) while paused at a breakpoint."
                .yellow()
        );
    }
}

fn repl_break(file: &str, line_strs: &[&str]) {
    let lines: Vec<u32> = line_strs
        .iter()
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    if lines.is_empty() {
        println!("No valid line numbers provided.");
        return;
    }

    let Some(path) = resolve_script_path(file) else {
        println!("{}", "Failed to resolve script path via daemon.".red());
        return;
    };

    let lines_json: Vec<serde_json::Value> = lines.iter().map(|&l| serde_json::json!(l)).collect();
    if let Some(body) = daemon_dap(
        "dap_set_breakpoints",
        serde_json::json!({"path": path, "lines": lines_json}),
    ) {
        let results = parse_breakpoint_results(&body);
        for bp in &results {
            let status = if bp.verified {
                "verified".green().to_string()
            } else {
                "unverified".yellow().to_string()
            };
            println!(
                "  {} {}:{} [{}]",
                "Breakpoint".bold(),
                file.cyan(),
                bp.line,
                status
            );
        }
    } else {
        println!("{}", "Failed to set breakpoints.".red());
    }
}

fn repl_clear(file: &str) {
    let Some(path) = resolve_script_path(file) else {
        println!("{}", "Failed to resolve script path via daemon.".red());
        return;
    };

    let empty: Vec<serde_json::Value> = vec![];
    if daemon_dap(
        "dap_set_breakpoints",
        serde_json::json!({"path": path, "lines": empty}),
    )
    .is_some()
    {
        println!("{} {}", "Cleared breakpoints in".green(), file.cyan());
    } else {
        println!("{}", "Failed to clear breakpoints.".red());
    }
}

fn repl_step_out() {
    let initial_depth = get_stack_frames().len();
    if initial_depth <= 1 {
        println!(
            "{}",
            "Cannot step out — already at the top-level frame.".yellow()
        );
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if daemon_dap("dap_next", serde_json::json!({})).is_none() {
            println!("{}", "Failed to step.".red());
            return;
        }
        if daemon_dap_timeout("dap_wait_stopped", serde_json::json!({"timeout": 5}), 5).is_none() {
            println!("{}", "Step-out timed out waiting for stop.".yellow());
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        if get_stack_frames().len() < initial_depth {
            println!("{}", "Stepped out".green());
            return;
        }
        if std::time::Instant::now() >= deadline {
            println!(
                "{}",
                "Step-out timed out after 15s — function may have a long-running loop.".yellow()
            );
            return;
        }
    }
}

fn repl_wait(timeout: u64) {
    println!(
        "{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        timeout
    );

    if daemon_dap_timeout(
        "dap_wait_stopped",
        serde_json::json!({"timeout": timeout}),
        timeout,
    )
    .is_some()
    {
        println!("{}", "Breakpoint hit!".green().bold());
        repl_stack();
        repl_vars(None);
    } else {
        println!(
            "{}",
            format!("Timeout — no breakpoint hit within {timeout}s.").yellow()
        );
    }
}

fn repl_scene_tree() {
    if let Some(result) = daemon_dap("debug_scene_tree", serde_json::json!({})) {
        println!("{}", "Scene tree:".bold());
        if let Some(nodes) = result.get("nodes").and_then(|n| n.as_array()) {
            for node in nodes {
                print_scene_node(node, 1);
            }
        } else if let Some(nodes) = result.as_array() {
            for node in nodes {
                print_scene_node(node, 1);
            }
        } else {
            print_scene_node(&result, 1);
        }
    } else {
        println!("{}", "Failed to get scene tree.".red());
    }
}

fn repl_inspect(id: u64) {
    if let Some(result) = daemon_dap("debug_inspect", serde_json::json!({"object_id": id})) {
        let class = result["class_name"].as_str().unwrap_or("Object");
        println!("{} {}", class.cyan().bold(), format!("(id: {id})").dimmed(),);
        println!("{}", "Properties:".bold());
        if let Some(props) = result["properties"].as_array() {
            if props.is_empty() {
                println!("  {}", "(none)".dimmed());
            }
            for p in props {
                let pname = p["name"].as_str().unwrap_or("?");
                let pval = format_variant_display(&p["value"]);
                println!("  {} = {}", pname.cyan(), pval.green());
            }
        } else {
            println!("  {}", "(no properties returned)".dimmed());
        }
    } else {
        println!("{}", "Failed to inspect object.".red());
    }
}

fn repl_set_prop(id: u64, property: &str, value: &str) {
    let json_value: serde_json::Value = serde_json::from_str(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

    if daemon_dap(
        "debug_set_property",
        serde_json::json!({
            "object_id": id,
            "property": property,
            "value": json_value,
        }),
    )
    .is_some()
    {
        println!(
            "{} {}.{} = {}",
            "Set".green(),
            format!("[{id}]").dimmed(),
            property.cyan(),
            value.green(),
        );
    } else {
        println!("{}", "Failed to set property.".red());
    }
}

fn repl_suspend(suspend: bool) {
    if daemon_dap("debug_suspend", serde_json::json!({"suspend": suspend})).is_some() {
        if suspend {
            println!("{}", "Game suspended".green());
        } else {
            println!("{}", "Game resumed".green());
        }
    } else {
        println!(
            "{}",
            if suspend {
                "Failed to suspend game.".red()
            } else {
                "Failed to resume game.".red()
            }
        );
    }
}

fn repl_next_frame() {
    if daemon_dap("debug_next_frame", serde_json::json!({})).is_some() {
        println!("{}", "Advanced one frame".green());
    } else {
        println!("{}", "Failed to advance frame.".red());
    }
}

fn repl_time_scale(scale: f64) {
    if daemon_dap("debug_time_scale", serde_json::json!({"scale": scale})).is_some() {
        println!("{}", format!("Time scale set to {scale}x").green());
    } else {
        println!("{}", "Failed to set time scale.".red());
    }
}

fn repl_reload_scripts() {
    if daemon_dap("debug_reload_scripts", serde_json::json!({})).is_some() {
        println!("{}", "Scripts reloaded".green());
    } else {
        println!("{}", "Failed to reload scripts.".red());
    }
}

fn repl_reload_all_scripts() {
    if daemon_dap("debug_reload_all_scripts", serde_json::json!({})).is_some() {
        println!("{}", "All scripts reloaded".green());
    } else {
        println!("{}", "Failed to reload all scripts.".red());
    }
}

fn repl_skip_breakpoints(off: bool) {
    let skip = !off;
    if daemon_dap(
        "debug_set_skip_breakpoints",
        serde_json::json!({"value": skip}),
    )
    .is_some()
    {
        if skip {
            println!("{}", "Breakpoints skipped".green());
        } else {
            println!("{}", "Breakpoints re-enabled".green());
        }
    } else {
        println!("{}", "Failed to toggle breakpoint skipping.".red());
    }
}

fn repl_ignore_errors(off: bool) {
    let ignore = !off;
    if daemon_dap(
        "debug_set_ignore_error_breaks",
        serde_json::json!({"value": ignore}),
    )
    .is_some()
    {
        if ignore {
            println!("{}", "Error breaks ignored".green());
        } else {
            println!("{}", "Error breaks re-enabled".green());
        }
    } else {
        println!("{}", "Failed to toggle error ignoring.".red());
    }
}

fn repl_mute_audio(mute: bool) {
    if daemon_dap("debug_mute_audio", serde_json::json!({"value": mute})).is_some() {
        if mute {
            println!("{}", "Audio muted".green());
        } else {
            println!("{}", "Audio unmuted".green());
        }
    } else {
        println!("{}", "Failed to toggle audio mute.".red());
    }
}

fn repl_override_camera(off: bool) {
    let enable = !off;
    if daemon_dap(
        "debug_override_camera",
        serde_json::json!({"enable": enable}),
    )
    .is_some()
    {
        if enable {
            println!("{}", "Camera override enabled".green());
        } else {
            println!("{}", "Camera override disabled".green());
        }
    } else {
        println!("{}", "Failed to toggle camera override.".red());
    }
}

fn repl_save_node(id: u64, path: &str) {
    if daemon_dap(
        "debug_save_node",
        serde_json::json!({"object_id": id, "path": path}),
    )
    .is_some()
    {
        println!(
            "{} node {} to {}",
            "Saved".green(),
            format!("[{id}]").dimmed(),
            path.cyan(),
        );
    } else {
        println!("{}", "Failed to save node.".red());
    }
}

fn repl_set_prop_field(id: u64, property: &str, field: &str, value: &str) {
    let json_value: serde_json::Value = serde_json::from_str(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

    if daemon_dap(
        "debug_set_property_field",
        serde_json::json!({
            "object_id": id,
            "property": property,
            "field": field,
            "value": json_value,
        }),
    )
    .is_some()
    {
        println!(
            "{} {}.{}.{} = {}",
            "Set".green(),
            format!("[{id}]").dimmed(),
            property.cyan(),
            field.cyan(),
            value.green(),
        );
    } else {
        println!("{}", "Failed to set property field.".red());
    }
}

fn repl_profiler(name: &str, off: bool) {
    let enable = !off;
    if daemon_dap(
        "debug_toggle_profiler",
        serde_json::json!({"name": name, "enable": enable}),
    )
    .is_some()
    {
        if enable {
            println!("{} profiler {}", "Enabled".green(), name.cyan());
        } else {
            println!("{} profiler {}", "Disabled".green(), name.cyan());
        }
    } else {
        println!("{}", "Failed to toggle profiler.".red());
    }
}

fn repl_live_set_root(path: &str, file: &str) {
    if daemon_dap(
        "debug_live_set_root",
        serde_json::json!({"scene_path": path, "scene_file": file}),
    )
    .is_some()
    {
        println!(
            "{} live root to {} {}",
            "Set".green(),
            path.cyan(),
            format!("({file})").dimmed(),
        );
    } else {
        println!("{}", "Failed to set live root.".red());
    }
}

fn repl_live_create_node(parent: &str, class: &str, name: &str) {
    if daemon_dap(
        "debug_live_create_node",
        serde_json::json!({"parent": parent, "class": class, "name": name}),
    )
    .is_some()
    {
        println!(
            "{} {} {}",
            "Created".green(),
            name.cyan(),
            format!("({class})").dimmed(),
        );
    } else {
        println!("{}", "Failed to create node.".red());
    }
}

fn repl_live_instantiate(parent: &str, scene: &str, name: &str) {
    if daemon_dap(
        "debug_live_instantiate_node",
        serde_json::json!({"parent": parent, "scene": scene, "name": name}),
    )
    .is_some()
    {
        println!(
            "{} {} {}",
            "Instantiated".green(),
            name.cyan(),
            format!("({scene})").dimmed(),
        );
    } else {
        println!("{}", "Failed to instantiate scene.".red());
    }
}

fn repl_live_remove_node(path: &str) {
    if daemon_dap("debug_live_remove_node", serde_json::json!({"path": path})).is_some() {
        println!("{} {}", "Removed".green(), path.cyan());
    } else {
        println!("{}", "Failed to remove node.".red());
    }
}

fn repl_live_duplicate(path: &str, name: &str) {
    if daemon_dap(
        "debug_live_duplicate_node",
        serde_json::json!({"path": path, "new_name": name}),
    )
    .is_some()
    {
        println!(
            "{} {} as {}",
            "Duplicated".green(),
            path.cyan(),
            name.cyan(),
        );
    } else {
        println!("{}", "Failed to duplicate node.".red());
    }
}

fn repl_live_reparent(path: &str, new_parent: &str, name: &str, pos: i32) {
    if daemon_dap(
        "debug_live_reparent_node",
        serde_json::json!({
            "path": path,
            "new_parent": new_parent,
            "new_name": name,
            "pos": pos,
        }),
    )
    .is_some()
    {
        println!(
            "{} {} to {}",
            "Reparented".green(),
            path.cyan(),
            new_parent.cyan(),
        );
    } else {
        println!("{}", "Failed to reparent node.".red());
    }
}

fn repl_live_node_prop(id: i32, property: &str, value: &str) {
    let json_value: serde_json::Value = serde_json::from_str(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));

    if daemon_dap(
        "debug_live_node_prop",
        serde_json::json!({"id": id, "property": property, "value": json_value}),
    )
    .is_some()
    {
        println!(
            "{} {}.{} = {}",
            "Set".green(),
            format!("[{id}]").dimmed(),
            property.cyan(),
            value.green(),
        );
    } else {
        println!("{}", "Failed to set live node property.".red());
    }
}

fn repl_live_node_call(id: i32, method: &str, args: &str) {
    let json_args: serde_json::Value =
        serde_json::from_str(args).unwrap_or_else(|_| serde_json::json!([]));

    if daemon_dap(
        "debug_live_node_call",
        serde_json::json!({"id": id, "method": method, "args": json_args}),
    )
    .is_some()
    {
        println!(
            "{} {}.{}({})",
            "Called".green(),
            format!("[{id}]").dimmed(),
            method.cyan(),
            args.dimmed(),
        );
    } else {
        println!("{}", "Failed to call method.".red());
    }
}

fn print_variables(
    vars: &[Variable],
    format: &OutputFormat,
    scope_name: Option<&str>,
) -> Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(vars).unwrap());
        }
        OutputFormat::Human => {
            if let Some(name) = scope_name {
                println!("\n{}", format!("{name}:").bold());
            }
            if vars.is_empty() {
                println!("  {}", "(empty)".dimmed());
            }
            for v in vars {
                let expand_hint = if v.variables_reference > 0 {
                    format!(" {}", format!("[ref={}]", v.variables_reference).dimmed())
                } else {
                    String::new()
                };
                if v.type_name.is_empty() {
                    println!("  {} = {}{}", v.name.cyan(), v.value.green(), expand_hint);
                } else {
                    println!(
                        "  {} {} = {}{}",
                        v.type_name.dimmed(),
                        v.name.cyan(),
                        v.value.green(),
                        expand_hint,
                    );
                }
            }
        }
    }
    Ok(())
}
