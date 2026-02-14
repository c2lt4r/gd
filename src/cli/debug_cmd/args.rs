use clap::{Args, Subcommand};

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
