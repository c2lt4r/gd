use std::fmt::Write as _;

use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;

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
    /// Hot-reload GDScript files in the running game
    #[command(name = "reload-scripts")]
    ReloadScripts(ReloadScriptsArgs),
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
    /// Live editing: set root scene (REQUIRED before live-node-prop/live-res-prop)
    ///
    /// Establishes the root mapping for live editing. All live-node-prop, live-res-prop,
    /// and live-*-call commands use live edit IDs assigned by this mapping — these are
    /// NOT the same as object IDs from scene-tree/inspect.
    ///
    /// Workflow: live-set-root → then use live-node-prop/live-res-prop with the IDs
    /// from this mapping. Use scene-tree object IDs for inspect/set-prop instead.
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
    /// Live editing: set a node property (uses live edit ID, not object ID)
    #[command(name = "live-node-prop")]
    LiveNodeProp(LiveNodePropArgs),
    /// Live editing: call a method on a node (uses live edit ID, not object ID)
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
    /// Structured spatial data: all visible nodes with positions, rotations, and camera info
    ///
    /// Returns JSON with camera transform and every spatial node's position/rotation/scale.
    /// Designed for AI reasoning about spatial relationships without needing screenshots.
    #[command(name = "camera-view")]
    CameraView(CameraViewArgs),
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
    /// Live editing: set node path mapping (uses live edit ID)
    #[command(name = "live-node-path")]
    LiveNodePath(LivePathArgs),
    /// Live editing: set resource path mapping (uses live edit ID)
    #[command(name = "live-res-path")]
    LiveResPath(LivePathArgs),
    /// Live editing: set resource property (uses live edit ID)
    #[command(name = "live-res-prop")]
    LiveResProp(LiveNodePropArgs),
    /// Live editing: set node property to resource (uses live edit ID)
    #[command(name = "live-node-prop-res")]
    LiveNodePropRes(LivePropResArgs),
    /// Live editing: set resource property to resource (uses live edit ID)
    #[command(name = "live-res-prop-res")]
    LiveResPropRes(LivePropResArgs),
    /// Live editing: call method on resource (uses live edit ID)
    #[command(name = "live-res-call")]
    LiveResCall(LiveNodeCallArgs),

    // ── Live editing: advanced node operations ──
    /// Live editing: remove node but keep reference (uses object ID)
    #[command(name = "live-remove-keep")]
    LiveRemoveKeep(LiveRemoveKeepArgs),
    /// Live editing: restore previously removed node (uses object ID)
    #[command(name = "live-restore")]
    LiveRestore(LiveRestoreArgs),

    /// Start the binary debug server and print the port (for manual testing)
    Server(ServerArgs),
}

#[derive(Args)]
pub struct StepArgs {
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ReloadScriptsArgs {
    /// Script paths to reload (e.g. res://player.gd). Reloads all if omitted.
    #[arg(long = "path")]
    pub paths: Vec<String>,
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
    /// Enrich output with ClassDB docs (property descriptions, class info)
    #[arg(long)]
    pub rich: bool,
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
    /// Take a screenshot after setting the property (outputs base64 PNG)
    #[arg(long)]
    pub screenshot: bool,
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
    /// Take a screenshot after setting the property (outputs base64 PNG)
    #[arg(long)]
    pub screenshot: bool,
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
    /// Scene path (e.g. "/root/Main") — maps to Godot's live edit root
    #[arg(long)]
    pub path: String,
    /// Scene file (e.g. "res://main.tscn") — the .tscn file for this scene
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
    /// Live edit ID (from live-set-root mapping — NOT the object ID from scene-tree)
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
    /// Live edit ID (from live-set-root mapping — NOT the object ID from scene-tree)
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
pub struct CameraViewArgs {
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
    /// Write PNG to file instead of printing base64 to stdout
    #[arg(long, short)]
    pub output: Option<String>,
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
    /// Live edit ID (from live-set-root mapping — NOT the object ID from scene-tree)
    #[arg(long)]
    pub id: i32,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LivePropResArgs {
    /// Live edit ID (from live-set-root mapping — NOT the object ID from scene-tree)
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

pub fn exec(args: &DebugArgs) -> Result<()> {
    match args.command {
        DebugCommand::Stop => crate::cli::stop_cmd::exec(),
        DebugCommand::SceneTree(ref a) => cmd_scene_tree(a),
        DebugCommand::Inspect(ref a) => cmd_inspect(a),
        DebugCommand::SetProp(ref a) => cmd_set_prop(a),
        DebugCommand::Suspend(ref a) => cmd_suspend(a),
        DebugCommand::NextFrame(ref a) => cmd_next_frame(a),
        DebugCommand::TimeScale(ref a) => cmd_time_scale(a),
        DebugCommand::ReloadScripts(ref a) => cmd_reload_scripts(a),
        DebugCommand::ReloadAllScripts(ref a) => cmd_reload_all_scripts(a),
        DebugCommand::SkipBreakpoints(ref a) => cmd_skip_breakpoints(a),
        DebugCommand::IgnoreErrors(ref a) => cmd_ignore_errors(a),
        DebugCommand::MuteAudio(ref a) => cmd_mute_audio(a),
        DebugCommand::OverrideCamera(ref a) => cmd_override_camera(a),
        DebugCommand::SaveNode(ref a) => cmd_save_node(a),
        DebugCommand::SetPropField(ref a) => cmd_set_prop_field(a),
        DebugCommand::Profiler(ref a) => cmd_profiler(a),
        DebugCommand::LiveSetRoot(ref a) => cmd_live_set_root(a),
        DebugCommand::LiveCreateNode(ref a) => cmd_live_create_node(a),
        DebugCommand::LiveInstantiate(ref a) => cmd_live_instantiate(a),
        DebugCommand::LiveRemoveNode(ref a) => cmd_live_remove_node(a),
        DebugCommand::LiveDuplicate(ref a) => cmd_live_duplicate(a),
        DebugCommand::LiveReparent(ref a) => cmd_live_reparent(a),
        DebugCommand::LiveNodeProp(ref a) => cmd_live_node_prop(a),
        DebugCommand::LiveNodeCall(ref a) => cmd_live_node_call(a),
        DebugCommand::Continue(ref a) => cmd_exec_continue(a),
        DebugCommand::Pause(ref a) => cmd_exec_pause(a),
        DebugCommand::Next(ref a) => cmd_exec_next(a),
        DebugCommand::StepIn(ref a) => cmd_exec_step_in(a),
        DebugCommand::StepOutFn(ref a) => cmd_exec_step_out(a),
        DebugCommand::Breakpoint(ref a) => cmd_breakpoint(a),
        DebugCommand::Stack(ref a) => cmd_stack(a),
        DebugCommand::Vars(ref a) => cmd_vars(a),
        DebugCommand::Eval(ref a) => cmd_evaluate(a),
        DebugCommand::InspectObjects(ref a) => cmd_inspect_objects(a),
        DebugCommand::CameraView(ref a) => cmd_camera_view(a),
        DebugCommand::TransformCamera2d(ref a) => cmd_transform_camera_2d(a),
        DebugCommand::TransformCamera3d(ref a) => cmd_transform_camera_3d(a),
        DebugCommand::Screenshot(ref a) => cmd_screenshot(a),
        DebugCommand::ReloadCached(ref a) => cmd_reload_cached(a),
        DebugCommand::NodeSelectType(ref a) => cmd_node_select_type(a),
        DebugCommand::NodeSelectMode(ref a) => cmd_node_select_mode(a),
        DebugCommand::NodeSelectVisible(ref a) => cmd_node_select_visible(a),
        DebugCommand::NodeSelectAvoidLocked(ref a) => cmd_node_select_avoid_locked(a),
        DebugCommand::NodeSelectPreferGroup(ref a) => cmd_node_select_prefer_group(a),
        DebugCommand::NodeSelectResetCam2d(ref a) => cmd_node_select_reset_cam_2d(a),
        DebugCommand::NodeSelectResetCam3d(ref a) => cmd_node_select_reset_cam_3d(a),
        DebugCommand::ClearSelection(ref a) => cmd_clear_selection(a),
        DebugCommand::LiveNodePath(ref a) => cmd_live_node_path(a),
        DebugCommand::LiveResPath(ref a) => cmd_live_res_path(a),
        DebugCommand::LiveResProp(ref a) => cmd_live_res_prop(a),
        DebugCommand::LiveNodePropRes(ref a) => cmd_live_node_prop_res(a),
        DebugCommand::LiveResPropRes(ref a) => cmd_live_res_prop_res(a),
        DebugCommand::LiveResCall(ref a) => cmd_live_res_call(a),
        DebugCommand::LiveRemoveKeep(ref a) => cmd_live_remove_keep(a),
        DebugCommand::LiveRestore(ref a) => cmd_live_restore(a),
        DebugCommand::Server(ref a) => cmd_server(a),
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
        let port = status.get("port").and_then(serde_json::Value::as_u64).unwrap_or(0);
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
    let port = result.get("port").and_then(serde_json::Value::as_u64).unwrap_or(0);

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

/// Try to enter the debug loop so evaluate works.
/// If already at a breakpoint, returns false (no action needed).
/// Otherwise sends `break` and waits for the game to pause.
/// Returns true if we auto-broke (caller should auto-continue after eval).
///
/// NOTE: `break` pauses the engine but may not provide GDScript context.
/// Breakpoints set on script lines provide full context for evaluate.
fn debug_break_for_eval() -> bool {
    if is_at_breakpoint() {
        return false;
    }
    let break_ok = daemon_cmd("debug_break_exec", serde_json::json!({}));
    if break_ok.is_none() {
        return false;
    }
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if is_at_breakpoint() {
            return true;
        }
    }
    // Break didn't pause within 1s — still return true so caller tries to continue
    true
}

// ── Server command ───────────────────────────────────────────────────

fn cmd_server(args: &ServerArgs) -> Result<()> {
    // Check if already running
    if let Some(status) = daemon_cmd("debug_server_status", serde_json::json!({}))
        && status.get("running").and_then(serde_json::Value::as_bool) == Some(true)
    {
        let port = status.get("port").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let connected = status.get("connected").and_then(serde_json::Value::as_bool) == Some(true);
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
        let accept = daemon_cmd_timeout(
            "debug_accept",
            serde_json::json!({"timeout": args.timeout}),
            args.timeout + 5,
        );
        if let Some(r) = accept
            && r.get("connected").and_then(serde_json::Value::as_bool) == Some(true)
        {
            println!("{}", "Game connected!".green().bold());
            return Ok(());
        }
        return Err(miette!("Timed out waiting for game to connect"));
    }

    // Start the server
    let result = daemon_cmd("debug_start_server", serde_json::json!({"port": args.port}))
        .ok_or_else(|| miette!("Failed to start debug server (daemon not available)"))?;
    let port = result.get("port").and_then(serde_json::Value::as_u64).unwrap_or(0);

    println!(
        "{} port {}",
        "Debug server started on".green().bold(),
        port.to_string().cyan(),
    );
    print_launch_hint(port);

    if args.wait {
        println!("{}", "Waiting for game to connect...".dimmed());
        let accept = daemon_cmd_timeout(
            "debug_accept",
            serde_json::json!({"timeout": args.timeout}),
            args.timeout + 5,
        );
        if let Some(r) = accept
            && r.get("connected").and_then(serde_json::Value::as_bool) == Some(true)
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

// ── Expression rewriting ─────────────────────────────────────────────
//
// Godot's evaluate uses the Expression class, not GDScript. It supports
// property reads, method calls, constructors, and built-in functions but
// NOT assignments, $NodePath, %UniqueName, or compound operators.
//
// We rewrite common GDScript patterns into Expression-compatible equivalents
// so users can type natural GDScript and have it "just work".

/// Rewrite a GDScript expression into one compatible with Godot's Expression class.
/// Returns `(rewritten_expr, was_rewritten)`.
fn rewrite_eval_expression(expr: &str) -> (String, bool) {
    let trimmed = expr.trim();

    // Already using set()/set_indexed() — pass through
    if trimmed.contains(".set(") || trimmed.contains(".set_indexed(") {
        return (trimmed.to_string(), false);
    }

    // 1. Semicolon-separated multi-expression → array trick (before assignment check,
    //    since individual parts may contain assignments that get rewritten recursively)
    if let Some(rewritten) = rewrite_multi_expression(trimmed) {
        return (rewritten, true);
    }

    // 2. $NodePath / %UniqueName rewrites (before assignment check)
    if let Some(rewritten) = rewrite_node_paths(trimmed) {
        return (rewritten, true);
    }

    // 3. Compound assignment: +=, -=, *=, /=
    if let Some(rewritten) = rewrite_compound_assignment(trimmed) {
        return (rewritten, true);
    }

    // 4. Simple assignment: lhs = rhs
    if let Some(rewritten) = rewrite_simple_assignment(trimmed) {
        return (rewritten, true);
    }

    (trimmed.to_string(), false)
}

/// Rewrite `$NodePath` → `get_node("NodePath")` and `%Unique` → `get_node("%Unique")`.
/// Handles `$Path.property`, `$"Quoted/Path"`, chained access, and method calls.
fn rewrite_node_paths(expr: &str) -> Option<String> {
    // Check if expression contains $ or % node references
    if !expr.contains('$') && !contains_unique_ref(expr) {
        return None;
    }

    let mut result = String::with_capacity(expr.len() + 16);
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // $"quoted/path" or $'quoted/path'
            if i + 1 < chars.len() && (chars[i + 1] == '"' || chars[i + 1] == '\'') {
                let quote = chars[i + 1];
                let start = i + 2;
                let mut end = start;
                while end < chars.len() && chars[end] != quote {
                    end += 1;
                }
                let path: String = chars[start..end].iter().collect();
                let _ = write!(result, "get_node(\"{path}\")");
                i = if end < chars.len() { end + 1 } else { end };
            } else {
                // $NodePath — consume identifier chars, /, and .. for parent refs
                let start = i + 1;
                let mut end = start;
                while end < chars.len() {
                    let c = chars[end];
                    if c.is_alphanumeric() || c == '_' || c == '/' {
                        end += 1;
                    } else if c == '.' {
                        // Allow ".." for parent paths ($../Sibling), but stop at
                        // single "." which is property access ($Player.speed)
                        if end + 1 < chars.len() && chars[end + 1] == '.' {
                            end += 2; // consume both dots
                        } else {
                            break; // single dot = property access
                        }
                    } else {
                        break;
                    }
                }
                if end > start {
                    let path: String = chars[start..end].iter().collect();
                    let _ = write!(result, "get_node(\"{path}\")");
                } else {
                    result.push('$');
                }
                i = end;
            }
        } else if chars[i] == '%' && is_unique_ref_at(&chars, i) {
            // %UniqueName
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end > start {
                let name: String = chars[start..end].iter().collect();
                let _ = write!(result, "get_node(\"%{name}\")");
            } else {
                result.push('%');
            }
            i = end;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    if result == expr {
        None
    } else {
        Some(result)
    }
}

/// Check if expression contains a `%UniqueName` reference (not `%` in modulo context).
fn contains_unique_ref(expr: &str) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    chars.iter().enumerate().any(|(i, _)| is_unique_ref_at(&chars, i))
}

/// Check if `%` at position `i` is a unique node reference (not modulo operator).
/// A `%` is a unique ref when it's at the start or preceded by whitespace/operator,
/// and followed by an identifier character.
fn is_unique_ref_at(chars: &[char], i: usize) -> bool {
    if chars[i] != '%' {
        return false;
    }
    // Must be followed by an identifier start character
    let next_is_ident = i + 1 < chars.len() && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_');
    if !next_is_ident {
        return false;
    }
    // At start of expression — it's a unique ref
    if i == 0 {
        return true;
    }
    // After whitespace, open paren, comma, operator — it's a unique ref
    let prev = chars[i - 1];
    prev.is_whitespace() || matches!(prev, '(' | ',' | '[' | '=' | '+' | '-' | '*' | '/' | '!')
}

/// Rewrite compound assignment: `lhs += rhs` → `set("lhs", lhs + rhs)`.
fn rewrite_compound_assignment(expr: &str) -> Option<String> {
    for (op_assign, op) in [("+=", "+"), ("-=", "-"), ("*=", "*"), ("/=", "/")] {
        if let Some(pos) = expr.find(op_assign) {
            let lhs = expr[..pos].trim();
            let rhs = expr[pos + op_assign.len()..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            return Some(build_set_expression(lhs, &format!("{lhs} {op} {rhs}")));
        }
    }
    None
}

/// Rewrite simple assignment: `lhs = rhs` → `set("lhs", rhs)`.
fn rewrite_simple_assignment(expr: &str) -> Option<String> {
    // Find `=` that isn't part of ==, !=, <=, >=, :=
    let bytes = expr.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        if next == b'=' {
            continue;
        }
        if matches!(prev, b'!' | b'<' | b'>' | b':' | b'+' | b'-' | b'*' | b'/' | b'=') {
            continue;
        }
        let lhs = expr[..i].trim();
        let rhs = expr[i + 1..].trim();
        if lhs.is_empty() || rhs.is_empty() {
            return None;
        }
        return Some(build_set_expression(lhs, rhs));
    }
    None
}

/// Build a `set()` or `set_indexed()` call from an assignment target and value.
///
/// - `speed = 10` → `set("speed", 10)`
/// - `self.speed = 10` → `set("speed", 10)`
/// - `position.x = 5` → `set_indexed("position:x", 5)`
/// - `self.position.x = 5` → `set_indexed("position:x", 5)`
fn build_set_expression(lhs: &str, rhs: &str) -> String {
    let prop = lhs.strip_prefix("self.").unwrap_or(lhs);

    // Nested property: position.x → set_indexed("position:x", value)
    if let Some(dot_pos) = prop.find('.') {
        let indexed_path = format!("{}:{}", &prop[..dot_pos], &prop[dot_pos + 1..]);
        format!("set_indexed(\"{indexed_path}\", {rhs})")
    } else {
        format!("set(\"{prop}\", {rhs})")
    }
}

/// Rewrite semicolon-separated expressions into an array (each element evaluates).
/// `print("hi"); speed = 10` → `[print("hi"), set("speed", 10)]`
fn rewrite_multi_expression(expr: &str) -> Option<String> {
    if !expr.contains(';') {
        return None;
    }

    // Split on semicolons outside of strings and parens
    let parts = split_on_semicolons(expr);
    if parts.len() < 2 {
        return None;
    }

    let rewritten: Vec<String> = parts
        .iter()
        .map(|part| {
            let (r, _) = rewrite_eval_expression(part.trim());
            r
        })
        .collect();

    Some(format!("[{}]", rewritten.join(", ")))
}

/// Split expression on semicolons, respecting string literals and nested parens/brackets.
fn split_on_semicolons(expr: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let bytes = expr.as_bytes();
    let mut depth = 0u32; // paren/bracket depth
    let mut in_string = false;
    let mut string_char = b'"';
    let mut start = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if b == string_char && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => {
                in_string = true;
                string_char = b;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => {
                parts.push(&expr[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&expr[start..]);
    parts
}

// ── One-shot: scene-tree ─────────────────────────────────────────────

fn cmd_scene_tree(args: &SceneTreeArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_scene_tree", serde_json::json!({}))
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
        "Bool" | "Int" | "Float" => val.map(std::string::ToString::to_string).unwrap_or_default(),
        "String" | "StringName" | "NodePath" => {
            val.and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
        "Vector2" | "Vector3" | "Vector4" | "Vector2i" | "Vector3i" | "Vector4i" | "Color"
        | "Rect2" | "Rect2i" | "Transform2D" | "Basis" | "Transform3D" | "Quaternion" | "AABB"
        | "Plane" | "Projection" => {
            if let Some(arr) = val.and_then(|v| v.as_array()) {
                let parts: Vec<String> = arr.iter().map(std::string::ToString::to_string).collect();
                format!("{typ}({})", parts.join(", "))
            } else {
                val.map(std::string::ToString::to_string).unwrap_or_default()
            }
        }
        "ObjectId" => val.map(|v| format!("Object#{v}")).unwrap_or_default(),
        _ => val.map_or_else(|| typ.to_string(), std::string::ToString::to_string),
    }
}

// ── One-shot: inspect ───────────────────────────────────────────────

fn cmd_inspect(args: &InspectArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_inspect", serde_json::json!({"object_id": args.id}))
        .ok_or_else(|| {
            miette!(
                "Failed to inspect object {} — is a game running with the binary debug protocol?",
                args.id
            )
        })?;

    if args.brief {
        return print_inspect_brief(&result, args.id, &args.format);
    }

    // Optionally enrich with ClassDB docs
    let result = if args.rich {
        crate::debug::enrich::enrich_inspect(&result)
    } else {
        result
    };

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
            // Show class docs if enriched
            if let Some(class_docs) = result.get("class_docs") {
                if let Some(brief) = class_docs["brief"].as_str() {
                    println!("  {}", brief.dimmed());
                }
                if let Some(url) = class_docs["docs_url"].as_str() {
                    println!("  {}", url.dimmed());
                }
            }
            println!("{}", "Properties:".bold());
            if let Some(props) = result["properties"].as_array() {
                if props.is_empty() {
                    println!("  {}", "(none)".dimmed());
                }
                for p in props {
                    let pname = p["name"].as_str().unwrap_or("?");
                    let pval = format_variant_display(&p["value"]);
                    if let Some(docs) = p.get("docs") {
                        let doc_brief = docs["brief"].as_str().unwrap_or("");
                        println!(
                            "  {} = {}  {}",
                            pname.cyan(),
                            pval.green(),
                            doc_brief.dimmed()
                        );
                    } else {
                        println!("  {} = {}", pname.cyan(), pval.green());
                    }
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
#[allow(clippy::unnecessary_wraps)]
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

fn cmd_set_prop(args: &SetPropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    let result = daemon_cmd(
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
            if args.screenshot {
                let (w, h, b64) = take_screenshot_b64()?;
                let mut combined = result.clone();
                combined["screenshot"] = serde_json::json!({
                    "width": w, "height": h, "format": "png", "data": b64,
                });
                println!("{}", serde_json::to_string_pretty(&combined).unwrap());
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
            if args.screenshot {
                let (_w, _h, b64) = take_screenshot_b64()?;
                print!("{b64}");
            }
        }
    }
    Ok(())
}

// ── One-shot: suspend ───────────────────────────────────────────────

fn cmd_suspend(args: &SuspendArgs) -> Result<()> {
    ensure_binary_debug()?;
    let suspend = !args.off;
    let result =
        daemon_cmd("debug_suspend", serde_json::json!({"suspend": suspend})).ok_or_else(|| {
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

fn cmd_next_frame(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_next_frame", serde_json::json!({}))
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

fn cmd_time_scale(args: &TimeScaleArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_time_scale", serde_json::json!({"scale": args.scale}))
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

fn cmd_reload_scripts(args: &ReloadScriptsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let params = if args.paths.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({"paths": args.paths})
    };
    let result = daemon_cmd("debug_reload_scripts", params).ok_or_else(|| {
        miette!("Failed to reload scripts — is a game running with the binary debug protocol?")
    })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if args.paths.is_empty() {
                println!("{}", "All scripts reloaded".green());
            } else {
                println!(
                    "{} {} script(s)",
                    "Reloaded".green(),
                    args.paths.len()
                );
            }
        }
    }
    Ok(())
}

// ── One-shot: reload-all-scripts ─────────────────────────────────────

fn cmd_reload_all_scripts(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_reload_all_scripts", serde_json::json!({}))
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

fn cmd_skip_breakpoints(args: &SkipBreakpointsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let skip = !args.off;
    daemon_cmd(
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

fn cmd_ignore_errors(args: &IgnoreErrorsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let ignore = !args.off;
    daemon_cmd(
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

fn cmd_mute_audio(args: &MuteAudioArgs) -> Result<()> {
    ensure_binary_debug()?;
    let mute = !args.off;
    daemon_cmd("debug_mute_audio", serde_json::json!({"mute": mute}))
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

fn cmd_override_camera(args: &OverrideCameraArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enable = !args.off;
    daemon_cmd(
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

fn cmd_save_node(args: &SaveNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_set_prop_field(args: &SetPropFieldArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_cmd(
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
            let mut out = serde_json::json!({
                "object_id": args.id,
                "property": args.property,
                "field": args.field,
                "value": json_value,
            });
            if args.screenshot {
                let (w, h, b64) = take_screenshot_b64()?;
                out["screenshot"] = serde_json::json!({
                    "width": w, "height": h, "format": "png", "data": b64,
                });
            }
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
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
            if args.screenshot {
                let (_w, _h, b64) = take_screenshot_b64()?;
                print!("{b64}");
            }
        }
    }
    Ok(())
}

// ── One-shot: profiler ──────────────────────────────────────────────

fn cmd_profiler(args: &ProfilerArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enable = !args.off;
    daemon_cmd(
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

fn cmd_live_set_root(args: &LiveSetRootArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_create_node(args: &LiveCreateNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_instantiate(args: &LiveInstantiateArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_remove_node(args: &LiveRemoveNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_duplicate(args: &LiveDuplicateArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_reparent(args: &LiveReparentArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_node_prop(args: &LiveNodePropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_cmd(
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

fn cmd_live_node_call(args: &LiveNodeCallArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_args: serde_json::Value =
        serde_json::from_str(&args.args).unwrap_or_else(|_| serde_json::json!([]));

    daemon_cmd(
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

fn cmd_exec_continue(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Send debugger continue (resumes from breakpoint)
    daemon_cmd("debug_continue", serde_json::json!({}));
    // Also unsuspend the scene tree and re-enable input (in case the game
    // was paused via suspend rather than a debugger breakpoint)
    daemon_cmd("debug_suspend", serde_json::json!({"suspend": false}));
    daemon_cmd("debug_node_select_set_type", serde_json::json!({"type": 0}));
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

fn cmd_exec_pause(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Use scene-level suspend (freezes game loop + disables input)
    // rather than debugger break (which halts script execution)
    daemon_cmd("debug_suspend", serde_json::json!({"suspend": true}))
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

fn cmd_exec_next(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_next_step", serde_json::json!({}))
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

fn cmd_exec_step_in(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_step_in", serde_json::json!({}))
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

fn cmd_exec_step_out(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_step_out", serde_json::json!({}))
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

fn cmd_breakpoint(args: &BreakpointBinArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enabled = !args.off;

    // Resolve --name to path:line if provided
    let (path, line) = if let Some(ref func_name) = args.name {
        let (p, l) = resolve_function_to_location(func_name)?;
        // --path/--line override --name if both given
        let path = args.path.clone().unwrap_or(p);
        let line = args.line.unwrap_or(l);
        (path, line)
    } else {
        let path = args
            .path
            .clone()
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
    daemon_cmd("debug_breakpoint", bp_params)
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

fn cmd_stack(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_get_stack_dump", serde_json::json!({}))
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

fn cmd_vars(args: &VarsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd(
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

fn cmd_evaluate(args: &EvalBinArgs) -> Result<()> {
    ensure_binary_debug()?;

    let input = args.expr.trim();
    let (expr, was_rewritten) = rewrite_eval_expression(input);
    if was_rewritten && !matches!(args.format, OutputFormat::Json) {
        eprintln!("  {} {}", "Rewritten:".dimmed(), expr.dimmed());
    }

    // Auto-break: send "break" to pause the script debugger, evaluate, then continue.
    // The binary protocol's evaluate only works inside Godot's debug() loop.
    let auto_broke = debug_break_for_eval();

    let result = daemon_cmd(
        "debug_evaluate",
        serde_json::json!({"expression": expr, "frame": args.frame}),
    );

    if auto_broke {
        daemon_cmd("debug_continue", serde_json::json!({}));
    }

    let result = result.ok_or_else(|| miette!("Evaluate failed — is a game running?"))?;

    match args.format {
        OutputFormat::Json => {
            let mut json = result.clone();
            if was_rewritten {
                json["rewritten_expression"] = serde_json::json!(expr);
                json["original_expression"] = serde_json::json!(input);
            }
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        OutputFormat::Human => {
            let display = format_variant_display(&result);
            if type_name_from_variant(&result).is_empty() {
                println!("{} = {}", input.cyan(), display.green());
            } else {
                println!(
                    "{} {} = {}",
                    type_name_from_variant(&result).dimmed(),
                    input.cyan(),
                    display.green()
                );
            }
        }
    }
    Ok(())
}

/// Extract the type name from a binary protocol variant result.
fn type_name_from_variant(v: &serde_json::Value) -> &str {
    v.get("type")
        .or_else(|| v.get("value").and_then(|val| val.get("type")))
        .and_then(|t| t.as_str())
        .unwrap_or("")
}

// ── Multi-object inspection (binary protocol) ───────────────────────

fn cmd_inspect_objects(args: &InspectObjectsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd(
        "debug_inspect_objects",
        serde_json::json!({"ids": args.id, "selection": args.selection}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            let objects = result.as_array().map_or(&[][..], std::vec::Vec::as_slice);
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

// ── Camera view: structured spatial data ─────────────────────────────
//
// Alternative approach not yet implemented: inject a temporary GDScript via
// reload-scripts that collects spatial data engine-side (frustum culling,
// physics layer info, etc). Would give true visibility data but is more
// invasive — modifies the project filesystem and risks game state changes.
// The current client-side batch approach (scene-tree + batch inspect) is
// non-invasive and sufficient for most AI debugging workflows.

#[allow(clippy::too_many_lines)]
fn cmd_camera_view(args: &CameraViewArgs) -> Result<()> {
    /// Check if a class is a known spatial type via the engine class DB.
    fn is_spatial_engine_class(class: &str) -> bool {
        class == "Node3D"
            || class == "Node2D"
            || crate::class_db::inherits(class, "Node3D")
            || crate::class_db::inherits(class, "Node2D")
    }

    /// Check if a class (engine name or script path) looks like a camera.
    /// Script paths like "res://scripts/player_camera.gd" use case-insensitive match.
    fn is_camera_class(class: &str, node_name: &str) -> bool {
        class == "Camera3D"
            || class == "Camera2D"
            || crate::class_db::inherits(class, "Camera3D")
            || crate::class_db::inherits(class, "Camera2D")
            || class.to_ascii_lowercase().contains("camera")
            || node_name.to_ascii_lowercase().contains("camera")
    }

    /// Script paths (res://...) aren't in class_db so we can't determine
    /// inheritance. Include them as spatial candidates — they'll be filtered
    /// after inspection based on whether they actually have transform properties.
    fn is_script_class(class: &str) -> bool {
        class.starts_with("res://")
    }

    fn walk_tree(
        node: &serde_json::Value,
        spatial_ids: &mut Vec<(u64, String, String)>,
        camera_ids: &mut Vec<(u64, String, String)>,
    ) {
        let name = node["name"].as_str().unwrap_or("").to_string();
        let class = node["class_name"].as_str().unwrap_or("").to_string();
        let id = node["object_id"].as_u64().unwrap_or(0);
        if id != 0 && !class.is_empty() {
            let camera = is_camera_class(&class, &name);
            if camera {
                camera_ids.push((id, name.clone(), class.clone()));
            }
            if is_spatial_engine_class(&class) || is_script_class(&class) || camera {
                spatial_ids.push((id, name, class));
            }
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                walk_tree(child, spatial_ids, camera_ids);
            }
        }
    }

    ensure_binary_debug()?;

    // Step 1: Get the scene tree
    let tree = daemon_cmd("debug_scene_tree", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to get scene tree — is a game running?"))?;

    // Step 2: Collect all spatial node IDs and find camera nodes
    let mut spatial_ids: Vec<(u64, String, String)> = Vec::new(); // (id, name, class)
    let mut camera_ids: Vec<(u64, String, String)> = Vec::new();

    // The tree may be a single root node or an array of nodes
    if let Some(nodes) = tree.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            walk_tree(node, &mut spatial_ids, &mut camera_ids);
        }
    } else if let Some(nodes) = tree.as_array() {
        for node in nodes {
            walk_tree(node, &mut spatial_ids, &mut camera_ids);
        }
    } else {
        walk_tree(&tree, &mut spatial_ids, &mut camera_ids);
    }

    if spatial_ids.is_empty() {
        return Err(miette!("No spatial nodes found in the scene tree"));
    }

    // Step 3: Batch inspect all spatial nodes
    let all_ids: Vec<u64> = spatial_ids.iter().map(|(id, _, _)| *id).collect();
    // Scale timeout: ~0.5s per node + 5s base, capped at 60s
    let inspect_timeout = (all_ids.len() as u64 / 2 + 5).min(60);
    let inspect_result = daemon_cmd_timeout(
        "debug_inspect_objects",
        serde_json::json!({"ids": all_ids, "selection": false}),
        inspect_timeout,
    )
    .ok_or_else(|| miette!("Failed to batch inspect spatial nodes"))?;

    let inspected = inspect_result
        .as_array()
        .map_or(&[][..], std::vec::Vec::as_slice);

    // Build lookup by object_id (responses may arrive out-of-order or be partial)
    let mut inspect_by_id: std::collections::HashMap<u64, &serde_json::Value> =
        std::collections::HashMap::new();
    for obj in inspected {
        if let Some(oid) = obj["object_id"].as_u64() {
            inspect_by_id.insert(oid, obj);
        }
    }

    // Step 4: Extract spatial properties from each inspected node
    let spatial_props = [
        "position",
        "global_position",
        "rotation",
        "rotation_degrees",
        "scale",
    ];
    let camera_props = ["fov", "size", "near", "far", "current", "projection"];

    let mut nodes_out: Vec<serde_json::Value> = Vec::new();
    let mut camera_out: Option<serde_json::Value> = None;

    for (id, name, class) in &spatial_ids {
        let obj = inspect_by_id.get(id);
        let mut node_data = serde_json::json!({
            "name": name,
            "class": class,
            "object_id": id,
        });

        let mut has_spatial = false;
        if let Some(obj) = obj
            && let Some(props) = obj["properties"].as_array()
        {
            let is_camera = camera_ids.iter().any(|(cid, _, _)| cid == id);
            for p in props {
                let pname = p["name"].as_str().unwrap_or("");
                if spatial_props.contains(&pname) {
                    node_data[pname] = format_spatial_value(&p["value"]);
                    has_spatial = true;
                } else if is_camera && camera_props.contains(&pname) {
                    node_data[pname] = format_spatial_value(&p["value"]);
                }
            }
        }

        // Script classes were included speculatively — drop if no spatial props
        if !has_spatial && is_script_class(class) {
            // Still check if it's a camera (cameras are useful even without transforms)
            if !camera_ids.iter().any(|(cid, _, _)| cid == id) {
                continue;
            }
        }

        // If this is a camera, also store as the camera info
        if camera_ids.iter().any(|(cid, _, _)| cid == id) {
            camera_out = Some(node_data.clone());
        }

        nodes_out.push(node_data);
    }

    let output = serde_json::json!({
        "camera": camera_out,
        "node_count": nodes_out.len(),
        "nodes": nodes_out,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Human => {
            if let Some(cam) = &camera_out {
                let cam_name = cam["name"].as_str().unwrap_or("?");
                let cam_class = cam["class"].as_str().unwrap_or("?");
                println!(
                    "{} {} {}",
                    "Camera:".bold(),
                    cam_name.cyan(),
                    format!("({cam_class})").dimmed(),
                );
                if let Some(pos) = cam.get("global_position") {
                    println!("  position: {}", format!("{pos}").green());
                }
                if let Some(rot) = cam.get("rotation_degrees").or_else(|| cam.get("rotation")) {
                    println!("  rotation: {}", format!("{rot}").green());
                }
                if let Some(fov) = cam.get("fov") {
                    println!("  fov: {}", format!("{fov}").green());
                }
                println!();
            } else {
                println!("{}", "No camera found in scene".dimmed());
                println!();
            }
            println!("{} ({} spatial nodes)", "Nodes:".bold(), nodes_out.len());
            for node in &nodes_out {
                let name = node["name"].as_str().unwrap_or("?");
                let class = node["class"].as_str().unwrap_or("?");
                let pos = node.get("global_position").or_else(|| node.get("position"));
                let rot = node
                    .get("rotation_degrees")
                    .or_else(|| node.get("rotation"));
                let pos_str = pos.map_or_else(|| "?".to_string(), |v| format!("{v}"));
                let rot_str = rot.map_or_else(|| "?".to_string(), |v| format!("{v}"));
                println!(
                    "  {} {} pos={} rot={}",
                    name.cyan(),
                    format!("({class})").dimmed(),
                    pos_str.green(),
                    rot_str.green(),
                );
            }
        }
    }
    Ok(())
}

/// Format a variant value for spatial display (simplify vectors to arrays).
fn format_spatial_value(value: &serde_json::Value) -> serde_json::Value {
    // Godot variants come as {"Vector3": [x,y,z]} — flatten to just [x,y,z]
    if let Some(obj) = value.as_object()
        && obj.len() == 1
        && let Some(inner) = obj.values().next()
        && inner.is_array()
    {
        return inner.clone();
    }
    value.clone()
}

// ── Camera transforms (binary protocol) ──────────────────────────────

fn cmd_transform_camera_2d(args: &TransformCamera2dArgs) -> Result<()> {
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
    daemon_cmd(
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

fn cmd_transform_camera_3d(args: &TransformCamera3dArgs) -> Result<()> {
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
    daemon_cmd(
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

/// Take a screenshot and return (width, height, base64_data).
/// Reused by `cmd_screenshot` and `--screenshot` flags on set-prop commands.
fn take_screenshot_b64() -> Result<(u64, u64, String)> {
    use base64::Engine;

    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1);
    let result = daemon_cmd("debug_request_screenshot", serde_json::json!({"id": id}))
        .ok_or_else(|| miette!("Screenshot failed — is a game running?"))?;

    let width = result["width"].as_u64().unwrap_or(0);
    let height = result["height"].as_u64().unwrap_or(0);
    let png_b64 = result["data"]
        .as_str()
        .ok_or_else(|| miette!("No screenshot data in response"))?;

    // Convert PNG → JPEG to reduce base64 size (~3-5x smaller)
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(png_b64)
        .map_err(|e| miette!("Failed to decode screenshot data: {e}"))?;
    let jpeg_b64 = png_to_jpeg_b64(&png_bytes)?;
    Ok((width, height, jpeg_b64))
}

/// Convert PNG bytes to JPEG, return as base64.
fn png_to_jpeg_b64(png_bytes: &[u8]) -> Result<String> {
    use base64::Engine;

    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| miette!("Failed to decode PNG: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| miette!("Failed to read PNG frame: {e}"))?;
    let pixels = &buf[..info.buffer_size()];
    let width = info.width as u16;
    let height = info.height as u16;

    // Convert to RGB if RGBA (strip alpha)
    let rgb_data = match info.color_type {
        png::ColorType::Rgba => {
            let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
            for chunk in pixels.chunks_exact(4) {
                rgb.extend_from_slice(&chunk[..3]);
            }
            rgb
        }
        png::ColorType::Rgb => pixels.to_vec(),
        other => return Err(miette!("Unsupported PNG color type: {other:?}")),
    };

    let mut jpeg_buf = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut jpeg_buf, 80);
    encoder
        .encode(&rgb_data, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| miette!("Failed to encode JPEG: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_buf))
}

/// Print screenshot as base64 JPEG (default) or write to file (PNG).
fn print_screenshot(
    b64_data: &str,
    width: u64,
    height: u64,
    output: Option<&str>,
    format: &OutputFormat,
) -> Result<()> {
    if let Some(output) = output {
        // --output writes JPEG to file (same as base64 output, just decoded to bytes)
        use base64::Engine;
        let img_bytes = base64::engine::general_purpose::STANDARD
            .decode(b64_data)
            .map_err(|e| miette!("Failed to decode screenshot data: {e}"))?;

        let fmt_label = "jpeg";
        let bytes_to_write = img_bytes;

        std::fs::write(output, &bytes_to_write)
            .map_err(|e| miette!("Failed to write screenshot to {output}: {e}"))?;

        match format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": output,
                        "width": width,
                        "height": height,
                        "format": fmt_label,
                        "size": bytes_to_write.len(),
                    }))
                    .unwrap()
                );
            }
            OutputFormat::Human => {
                let size_kb = bytes_to_write.len() / 1024;
                println!(
                    "{} {}x{} ({size_kb} KB) → {}",
                    "Screenshot saved".green(),
                    width,
                    height,
                    output.cyan(),
                );
            }
        }
        return Ok(());
    }

    // Default: output JPEG base64 to stdout
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "width": width,
                    "height": height,
                    "format": "jpeg",
                    "data": b64_data,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            print!("{b64_data}");
        }
    }
    Ok(())
}

fn cmd_screenshot(args: &ScreenshotArgs) -> Result<()> {
    ensure_binary_debug()?;
    let (width, height, b64_data) = take_screenshot_b64()?;
    print_screenshot(
        &b64_data,
        width,
        height,
        args.output.as_deref(),
        &args.format,
    )
}

// ── File management (binary protocol) ───────────────────────────────

fn cmd_reload_cached(args: &ReloadCachedArgs) -> Result<()> {
    ensure_binary_debug()?;
    let count = args.file.len();
    daemon_cmd(
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

fn cmd_node_select_type(args: &NodeSelectIntArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_node_select_mode(args: &NodeSelectIntArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_node_select_visible(args: &ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let visible = !args.off;
    daemon_cmd(
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

fn cmd_node_select_avoid_locked(args: &ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let avoid = !args.off;
    daemon_cmd(
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

fn cmd_node_select_prefer_group(args: &ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let prefer = !args.off;
    daemon_cmd(
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

fn cmd_node_select_reset_cam_2d(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_node_select_reset_camera_2d", serde_json::json!({}))
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

fn cmd_node_select_reset_cam_3d(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_node_select_reset_camera_3d", serde_json::json!({}))
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

fn cmd_clear_selection(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_clear_selection", serde_json::json!({}))
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

fn cmd_live_node_path(args: &LivePathArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_res_path(args: &LivePathArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_res_prop(args: &LiveNodePropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_cmd(
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

fn cmd_live_node_prop_res(args: &LivePropResArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_res_prop_res(args: &LivePropResArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_res_call(args: &LiveNodeCallArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_args: serde_json::Value =
        serde_json::from_str(&args.args).unwrap_or_else(|_| serde_json::json!([]));

    daemon_cmd(
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

fn cmd_live_remove_keep(args: &LiveRemoveKeepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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

fn cmd_live_restore(args: &LiveRestoreArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
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



#[cfg(test)]
mod tests {
    use super::*;

    // ── rewrite_eval_expression ──────────────────────────────────────

    #[test]
    fn passthrough_simple_expression() {
        let (result, rewritten) = rewrite_eval_expression("position.x");
        assert_eq!(result, "position.x");
        assert!(!rewritten);
    }

    #[test]
    fn passthrough_method_call() {
        let (result, rewritten) = rewrite_eval_expression("get_node(\"Player\").get_name()");
        assert_eq!(result, "get_node(\"Player\").get_name()");
        assert!(!rewritten);
    }

    #[test]
    fn passthrough_comparison() {
        let (result, rewritten) = rewrite_eval_expression("speed == 10");
        assert_eq!(result, "speed == 10");
        assert!(!rewritten);
    }

    #[test]
    fn passthrough_set_call() {
        let (result, rewritten) = rewrite_eval_expression("self.set(\"speed\", 10)");
        assert_eq!(result, "self.set(\"speed\", 10)");
        assert!(!rewritten);
    }

    // ── Simple assignment rewrites ──────────────────────────────────

    #[test]
    fn rewrite_simple_assignment() {
        let (result, rewritten) = rewrite_eval_expression("speed = 10");
        assert_eq!(result, "set(\"speed\", 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_self_prefixed_assignment() {
        let (result, rewritten) = rewrite_eval_expression("self.speed = 10");
        assert_eq!(result, "set(\"speed\", 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_nested_property_assignment() {
        let (result, rewritten) = rewrite_eval_expression("position.x = 5");
        assert_eq!(result, "set_indexed(\"position:x\", 5)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_self_nested_property_assignment() {
        let (result, rewritten) = rewrite_eval_expression("self.position.x = 5.0");
        assert_eq!(result, "set_indexed(\"position:x\", 5.0)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_assignment_with_constructor() {
        let (result, rewritten) = rewrite_eval_expression("position = Vector3(1, 2, 3)");
        assert_eq!(result, "set(\"position\", Vector3(1, 2, 3))");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_assignment_with_bool() {
        let (result, rewritten) = rewrite_eval_expression("visible = false");
        assert_eq!(result, "set(\"visible\", false)");
        assert!(rewritten);
    }

    // ── Compound assignment rewrites ────────────────────────────────

    #[test]
    fn rewrite_plus_equals() {
        let (result, rewritten) = rewrite_eval_expression("speed += 10");
        assert_eq!(result, "set(\"speed\", speed + 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_minus_equals() {
        let (result, rewritten) = rewrite_eval_expression("health -= 25");
        assert_eq!(result, "set(\"health\", health - 25)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_times_equals() {
        let (result, rewritten) = rewrite_eval_expression("score *= 2");
        assert_eq!(result, "set(\"score\", score * 2)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_divide_equals() {
        let (result, rewritten) = rewrite_eval_expression("speed /= 2.0");
        assert_eq!(result, "set(\"speed\", speed / 2.0)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_self_compound_assignment() {
        let (result, rewritten) = rewrite_eval_expression("self.speed += 5");
        assert_eq!(result, "set(\"speed\", self.speed + 5)");
        assert!(rewritten);
    }

    // ── $NodePath rewrites ──────────────────────────────────────────

    #[test]
    fn rewrite_dollar_node() {
        let (result, rewritten) = rewrite_eval_expression("$Player");
        assert_eq!(result, "get_node(\"Player\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_nested_path() {
        let (result, rewritten) = rewrite_eval_expression("$Player/Sprite");
        assert_eq!(result, "get_node(\"Player/Sprite\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_quoted_path() {
        let (result, rewritten) = rewrite_eval_expression("$\"Path/With Spaces\"");
        assert_eq!(result, "get_node(\"Path/With Spaces\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_parent_path() {
        let (result, rewritten) = rewrite_eval_expression("$../Sibling");
        assert_eq!(result, "get_node(\"../Sibling\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_property_access() {
        let (result, rewritten) = rewrite_eval_expression("$Player.speed");
        assert_eq!(result, "get_node(\"Player\").speed");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_method_call() {
        let (result, rewritten) = rewrite_eval_expression("$Player.get_name()");
        assert_eq!(result, "get_node(\"Player\").get_name()");
        assert!(rewritten);
    }

    // ── %UniqueName rewrites ────────────────────────────────────────

    #[test]
    fn rewrite_unique_name() {
        let (result, rewritten) = rewrite_eval_expression("%Player");
        assert_eq!(result, "get_node(\"%Player\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_unique_name_property() {
        let (result, rewritten) = rewrite_eval_expression("%Player.speed");
        assert_eq!(result, "get_node(\"%Player\").speed");
        assert!(rewritten);
    }

    #[test]
    fn no_rewrite_modulo_operator() {
        let (result, rewritten) = rewrite_eval_expression("10 % 3");
        assert_eq!(result, "10 % 3");
        assert!(!rewritten);
    }

    // ── Multi-expression (semicolons) ───────────────────────────────

    #[test]
    fn rewrite_semicolons() {
        let (result, rewritten) = rewrite_eval_expression("print(\"hi\"); speed = 10");
        assert_eq!(result, "[print(\"hi\"), set(\"speed\", 10)]");
        assert!(rewritten);
    }

    #[test]
    fn no_rewrite_semicolon_in_string() {
        let (result, rewritten) = rewrite_eval_expression("\"hello; world\"");
        assert_eq!(result, "\"hello; world\"");
        assert!(!rewritten);
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn no_rewrite_not_equal() {
        let (result, rewritten) = rewrite_eval_expression("speed != 0");
        assert_eq!(result, "speed != 0");
        assert!(!rewritten);
    }

    #[test]
    fn no_rewrite_less_equal() {
        let (result, rewritten) = rewrite_eval_expression("speed <= 100");
        assert_eq!(result, "speed <= 100");
        assert!(!rewritten);
    }

    #[test]
    fn no_rewrite_greater_equal() {
        let (result, rewritten) = rewrite_eval_expression("speed >= 0");
        assert_eq!(result, "speed >= 0");
        assert!(!rewritten);
    }

    #[test]
    fn rewrite_whitespace_handling() {
        let (result, rewritten) = rewrite_eval_expression("  speed  =  10  ");
        assert_eq!(result, "set(\"speed\", 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_complex_rhs() {
        let (result, rewritten) = rewrite_eval_expression("speed = clamp(speed + 10, 0, 100)");
        assert_eq!(result, "set(\"speed\", clamp(speed + 10, 0, 100))");
        assert!(rewritten);
    }

    // ── split_on_semicolons ─────────────────────────────────────────

    #[test]
    fn split_simple() {
        let parts = split_on_semicolons("a; b; c");
        assert_eq!(parts, vec!["a", " b", " c"]);
    }

    #[test]
    fn split_respects_strings() {
        let parts = split_on_semicolons("\"a;b\"; c");
        assert_eq!(parts, vec!["\"a;b\"", " c"]);
    }

    #[test]
    fn split_respects_parens() {
        let parts = split_on_semicolons("f(a; b); c");
        // semicolons inside parens don't split (even though invalid GDScript)
        assert_eq!(parts, vec!["f(a; b)", " c"]);
    }

    // ── build_set_expression ────────────────────────────────────────

    #[test]
    fn build_set_simple() {
        assert_eq!(build_set_expression("speed", "10"), "set(\"speed\", 10)");
    }

    #[test]
    fn build_set_indexed() {
        assert_eq!(
            build_set_expression("position.x", "5"),
            "set_indexed(\"position:x\", 5)"
        );
    }

    #[test]
    fn build_set_strips_self() {
        assert_eq!(
            build_set_expression("self.health", "100"),
            "set(\"health\", 100)"
        );
    }
}
