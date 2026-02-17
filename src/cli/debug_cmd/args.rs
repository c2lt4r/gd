use clap::{Args, Subcommand};

#[derive(Args)]
pub struct DebugArgs {
    #[command(subcommand)]
    pub command: DebugCommand,
}

#[derive(Subcommand)]
pub enum DebugCommand {
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

    // ── Properties ──
    /// Set a property on a scene node by object ID
    #[command(name = "set-prop")]
    SetProp(SetPropArgs),
    /// Set a specific field within a property (e.g. position.x)
    #[command(name = "set-prop-field")]
    SetPropField(SetPropFieldArgs),

    // ── Game loop control ──
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
    /// Toggle a profiler (scripts, visual, servers)
    Profiler(ProfilerArgs),
    /// Save a scene node to a file
    #[command(name = "save-node")]
    SaveNode(SaveNodeArgs),
    /// Reload cached files
    #[command(name = "reload-cached")]
    ReloadCached(ReloadCachedArgs),

    // ── Input automation (requires eval server — enabled by default in `gd run`) ──
    /// Click at coordinates or on a named Control node
    Click(ClickArgs),
    /// Trigger a Godot input action (e.g. "ui_accept", "jump")
    Press(PressArgs),
    /// Press a keyboard key
    Key(KeyArgs),
    /// Type a string of text as key events
    Type(TypeTextArgs),
    /// Wait for a duration (between input actions)
    Wait(WaitArgs),
    /// Take a screenshot (alias for `camera screenshot`)
    Screenshot(ScreenshotArgs),

    // ── Node automation (requires eval server — enabled by default in `gd run`) ──
    /// Find nodes in the running scene by name, type, or group
    Find(FindArgs),
    /// Read a property value from a node by name/path/ID
    #[command(name = "get-prop")]
    GetProp(GetPropArgs),
    /// Call a method on a node by name/path/ID
    Call(CallArgs),
    /// Set a property on a node by name/path (no object ID needed)
    Set(SetNodeArgs),
    /// Wait for a runtime condition (node exists, property value, etc.)
    Await(AwaitArgs),
    /// AI-readable snapshot of game state: player position, nearby nodes, scene, input actions
    Describe(DescribeArgs),
    /// Navigate a node to a target position using its NavigationAgent
    Navigate(NavigateArgs),

    /// Move mouse cursor to screen coordinates or a node's position
    #[command(name = "mouse-move")]
    MouseMove(MoveToArgs),
    /// Drag mouse cursor from one position/node to another
    #[command(name = "mouse-drag")]
    MouseDrag(DragArgs),
    /// Hover mouse cursor over a node or position (triggers mouse_enter events)
    #[command(name = "mouse-hover")]
    MouseHover(HoverArgs),

    // ── Subcommand groups ──
    /// Live editing commands (requires `live set-root` first)
    Live(LiveArgs),
    /// Scene inspection commands
    Scene(SceneGroupArgs),
    /// Camera and screenshot commands
    Camera(CameraGroupArgs),
    /// Node selection commands
    Select(SelectArgs),

    /// Start the binary debug server and print the port (for manual testing)
    Server(ServerArgs),
}

// ── Live editing subcommand group ────────────────────────────────────

#[derive(Args)]
pub struct LiveArgs {
    #[command(subcommand)]
    pub command: LiveCommand,
}

#[derive(Subcommand)]
pub enum LiveCommand {
    /// Set root scene (REQUIRED before other live commands)
    ///
    /// Establishes the root mapping for live editing. All node-prop, res-prop,
    /// and *-call commands use live edit IDs assigned by this mapping — these are
    /// NOT the same as object IDs from scene tree/inspect.
    ///
    /// Workflow: set-root → then use node-prop/res-prop with the IDs
    /// from this mapping. Use scene tree object IDs for inspect/set-prop instead.
    #[command(name = "set-root")]
    SetRoot(LiveSetRootArgs),
    /// Create a new node
    #[command(name = "create-node")]
    CreateNode(LiveCreateNodeArgs),
    /// Instantiate a scene
    Instantiate(LiveInstantiateArgs),
    /// Remove a node
    #[command(name = "remove-node")]
    RemoveNode(LiveRemoveNodeArgs),
    /// Duplicate a node
    Duplicate(LiveDuplicateArgs),
    /// Reparent a node
    Reparent(LiveReparentArgs),
    /// Set a node property (uses live edit ID, not object ID)
    #[command(name = "node-prop")]
    NodeProp(LiveNodePropArgs),
    /// Call a method on a node (uses live edit ID, not object ID)
    #[command(name = "node-call")]
    NodeCall(LiveNodeCallArgs),
    /// Set node path mapping (uses live edit ID)
    #[command(name = "node-path")]
    NodePath(LivePathArgs),
    /// Set resource path mapping (uses live edit ID)
    #[command(name = "res-path")]
    ResPath(LivePathArgs),
    /// Set resource property (uses live edit ID)
    #[command(name = "res-prop")]
    ResProp(LiveNodePropArgs),
    /// Set node property to resource (uses live edit ID)
    #[command(name = "node-prop-res")]
    NodePropRes(LivePropResArgs),
    /// Set resource property to resource (uses live edit ID)
    #[command(name = "res-prop-res")]
    ResPropRes(LivePropResArgs),
    /// Call method on resource (uses live edit ID)
    #[command(name = "res-call")]
    ResCall(LiveNodeCallArgs),
    /// Remove node but keep reference (uses object ID)
    #[command(name = "remove-keep")]
    RemoveKeep(LiveRemoveKeepArgs),
    /// Restore previously removed node (uses object ID)
    Restore(LiveRestoreArgs),
}

// ── Scene inspection subcommand group ────────────────────────────────

#[derive(Args)]
pub struct SceneGroupArgs {
    #[command(subcommand)]
    pub command: SceneGroupCommand,
}

#[derive(Subcommand)]
pub enum SceneGroupCommand {
    /// Show the running game's scene tree
    Tree(SceneTreeArgs),
    /// Inspect a scene node's properties by object ID
    Inspect(InspectArgs),
    /// Inspect multiple objects
    #[command(name = "inspect-objects")]
    InspectObjects(InspectObjectsArgs),
    /// Structured spatial data: all visible nodes with positions, rotations, and camera info
    ///
    /// Returns JSON with camera transform and every spatial node's position/rotation/scale.
    /// Designed for AI reasoning about spatial relationships without needing screenshots.
    #[command(name = "camera-view")]
    CameraView(CameraViewArgs),
}

// ── Camera subcommand group ──────────────────────────────────────────

#[derive(Args)]
pub struct CameraGroupArgs {
    #[command(subcommand)]
    pub command: CameraGroupCommand,
}

#[derive(Subcommand)]
pub enum CameraGroupCommand {
    /// Override the game camera (take remote control)
    Override(OverrideCameraArgs),
    /// Transform 2D camera
    #[command(name = "transform-2d")]
    Transform2d(TransformCamera2dArgs),
    /// Transform 3D camera
    #[command(name = "transform-3d")]
    Transform3d(TransformCamera3dArgs),
    /// Request screenshot
    Screenshot(ScreenshotArgs),
}

// ── Node selection subcommand group ──────────────────────────────────

#[derive(Args)]
pub struct SelectArgs {
    #[command(subcommand)]
    pub command: SelectCommand,
}

#[derive(Subcommand)]
pub enum SelectCommand {
    /// Set selection type
    Type(NodeSelectIntArgs),
    /// Set selection mode
    Mode(NodeSelectIntArgs),
    /// Toggle visibility filter
    Visible(ToggleFmtArgs),
    /// Toggle avoid locked
    #[command(name = "avoid-locked")]
    AvoidLocked(ToggleFmtArgs),
    /// Toggle prefer group
    #[command(name = "prefer-group")]
    PreferGroup(ToggleFmtArgs),
    /// Reset 2D selection camera
    #[command(name = "reset-cam-2d")]
    ResetCam2d(StepArgs),
    /// Reset 3D selection camera
    #[command(name = "reset-cam-3d")]
    ResetCam3d(StepArgs),
    /// Clear selection
    Clear(StepArgs),
}

#[derive(Args)]
pub struct StepArgs {
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ReloadScriptsArgs {
    /// Script paths to reload (e.g. res://player.gd). Reloads all if omitted.
    #[arg(long = "path")]
    pub paths: Vec<String>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SceneTreeArgs {
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    /// Take a screenshot after setting the property (outputs PNG path)
    #[arg(long)]
    pub screenshot: bool,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SuspendArgs {
    /// Resume instead of suspend
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct TimeScaleArgs {
    /// Time scale (1.0 = normal, 0.5 = half speed, 2.0 = double speed)
    #[arg(long)]
    pub scale: f64,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SkipBreakpointsArgs {
    /// Disable skipping (re-enable breakpoints)
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct IgnoreErrorsArgs {
    /// Stop ignoring errors
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct MuteAudioArgs {
    /// Unmute instead
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct OverrideCameraArgs {
    /// Disable camera override
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    /// Take a screenshot after setting the property (outputs PNG path)
    #[arg(long)]
    pub screenshot: bool,
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct LiveRemoveNodeArgs {
    /// Node path to remove
    #[arg(long)]
    pub path: String,
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct VarsArgs {
    /// Stack frame index (default: 0 = top frame)
    #[arg(long, default_value = "0")]
    pub frame: u32,
    /// Output format
    #[arg(long, default_value = "text")]
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
    /// Use Godot's Expression class instead of file-based eval (limited to expressions,
    /// no loops/if/var — but can read local variables at a breakpoint)
    #[arg(long)]
    pub bare: bool,
    /// Timeout in seconds (default: 10)
    #[arg(short, long, default_value_t = 10)]
    pub timeout: u64,
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct CameraViewArgs {
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct TransformCamera2dArgs {
    /// Transform as JSON array of 6 floats
    #[arg(long)]
    pub transform: String,
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ScreenshotArgs {
    /// Copy PNG to this path (default: prints temp file path)
    #[arg(long, short)]
    pub output: Option<String>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ReloadCachedArgs {
    /// File paths to reload (repeatable)
    #[arg(long, num_args = 1..)]
    pub file: Vec<String>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct NodeSelectIntArgs {
    /// Integer value
    #[arg(long)]
    pub value: i32,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ToggleFmtArgs {
    /// Disable instead of enable
    #[arg(long)]
    pub off: bool,
    /// Output format
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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
    #[arg(long, default_value = "text")]
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

// ── Input automation args ─────────────────────────────────────────────

#[derive(Args)]
pub struct ClickArgs {
    /// Viewport coordinates (e.g. "100,200")
    #[arg(long)]
    pub pos: Option<String>,
    /// Node name (find_child) or path (/root/UI/Button)
    #[arg(long)]
    pub node: Option<String>,
    /// Mouse button: left, right, or middle
    #[arg(long, default_value = "left")]
    pub button: String,
    /// Double-click
    #[arg(long)]
    pub double: bool,
    /// Hold duration in seconds (for long-press / drag-start)
    #[arg(long)]
    pub hold: Option<f64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct PressArgs {
    /// Godot input action name (e.g. "ui_accept", "jump")
    #[arg(long)]
    pub action: String,
    /// Hold duration in seconds (default: instant press+release)
    #[arg(long)]
    pub hold: Option<f64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct KeyArgs {
    /// Key name (e.g. space, enter, a, f1, shift)
    #[arg(long)]
    pub key: String,
    /// Hold duration in seconds (default: instant press+release)
    #[arg(long)]
    pub hold: Option<f64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct TypeTextArgs {
    /// Text to type as key events
    #[arg(long)]
    pub text: String,
    /// Delay between characters in milliseconds
    #[arg(long)]
    pub delay: Option<u64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct WaitArgs {
    /// Wait N frames (~N*16.67ms)
    #[arg(long)]
    pub frames: Option<u64>,
    /// Wait N seconds
    #[arg(long)]
    pub seconds: Option<f64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

// ── Node automation args ──────────────────────────────────────────────

#[derive(Args)]
pub struct FindArgs {
    /// Node name (recursive find_child) or absolute path (/root/Main/Player)
    #[arg(long)]
    pub name: Option<String>,
    /// Find all nodes of this class type (e.g. "CharacterBody2D")
    #[arg(long, name = "type")]
    pub type_: Option<String>,
    /// Find all nodes in this group (e.g. "enemies")
    #[arg(long)]
    pub group: Option<String>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct GetPropArgs {
    /// Node name (find_child) or absolute path (/root/...)
    #[arg(long)]
    pub node: Option<String>,
    /// Object ID (from scene tree)
    #[arg(long)]
    pub id: Option<u64>,
    /// Property name to read
    #[arg(long)]
    pub property: String,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct CallArgs {
    /// Node name (find_child) or absolute path (/root/...)
    #[arg(long)]
    pub node: Option<String>,
    /// Object ID (from scene tree)
    #[arg(long)]
    pub id: Option<u64>,
    /// Method name to call
    #[arg(long)]
    pub method: String,
    /// Arguments as JSON array (e.g. '[10, "hello"]')
    #[arg(long, default_value = "[]")]
    pub args: String,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SetNodeArgs {
    /// Node name (find_child) or absolute path (/root/...)
    #[arg(long)]
    pub node: String,
    /// Property name to set
    #[arg(long)]
    pub property: String,
    /// Value as GDScript expression (e.g. "200", "Vector2(100, 200)", '"Game Over"')
    #[arg(long)]
    pub value: String,
    /// Take a screenshot after setting the property
    #[arg(long)]
    pub screenshot: bool,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct AwaitArgs {
    /// Node name/path to watch (for --property or --removed)
    #[arg(long)]
    pub node: Option<String>,
    /// Property name to poll
    #[arg(long)]
    pub property: Option<String>,
    /// Wait until property equals this value
    #[arg(long)]
    pub equals: Option<String>,
    /// Wait until property is greater than this value
    #[arg(long)]
    pub gt: Option<String>,
    /// Wait until property is less than this value
    #[arg(long)]
    pub lt: Option<String>,
    /// Wait until property string contains this substring
    #[arg(long)]
    pub contains: Option<String>,
    /// Wait for node to be removed (instead of existing)
    #[arg(long)]
    pub removed: bool,
    /// Timeout in seconds (default: 10)
    #[arg(long, default_value = "10")]
    pub timeout: f64,
    /// Poll interval in milliseconds (default: 200)
    #[arg(long, default_value = "200")]
    pub interval: u64,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct DescribeArgs {
    /// Node to use as the reference point (default: auto-detect player)
    #[arg(long)]
    pub node: Option<String>,
    /// Radius for nearby node search (default: 500 for 2D, 20 for 3D)
    #[arg(long)]
    pub radius: Option<f64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct NavigateArgs {
    /// Node to move (must have a NavigationAgent2D/3D child)
    #[arg(long)]
    pub node: String,
    /// Target coordinates (e.g. "500,300" for 2D or "10,0,5" for 3D)
    #[arg(long)]
    pub to: Option<String>,
    /// Target node name/path (navigates to its position)
    #[arg(long)]
    pub to_node: Option<String>,
    /// Timeout in seconds (default: 30)
    #[arg(long, default_value = "30")]
    pub timeout: f64,
    /// Poll interval in milliseconds (default: 200)
    #[arg(long, default_value = "200")]
    pub interval: u64,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct MoveToArgs {
    /// Viewport coordinates (e.g. "400,300")
    #[arg(long)]
    pub pos: Option<String>,
    /// Node name (find_child) or absolute path
    #[arg(long)]
    pub node: Option<String>,
    /// Smooth move duration in seconds (interpolated steps)
    #[arg(long)]
    pub duration: Option<f64>,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct DragArgs {
    /// Start coordinates (e.g. "100,200")
    #[arg(long)]
    pub from: Option<String>,
    /// End coordinates (e.g. "300,400")
    #[arg(long)]
    pub to: Option<String>,
    /// Start node name/path
    #[arg(long)]
    pub from_node: Option<String>,
    /// End node name/path
    #[arg(long)]
    pub to_node: Option<String>,
    /// Mouse button: left, right, or middle
    #[arg(long, default_value = "left")]
    pub button: String,
    /// Drag duration in seconds (default: 0.2)
    #[arg(long, default_value = "0.2")]
    pub duration: f64,
    /// Number of interpolation steps (default: 10)
    #[arg(long, default_value = "10")]
    pub steps: u32,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct HoverArgs {
    /// Node name (find_child) or absolute path
    #[arg(long)]
    pub node: Option<String>,
    /// Viewport coordinates (e.g. "200,150")
    #[arg(long)]
    pub pos: Option<String>,
    /// How long to hold the hover in seconds (default: 0.1)
    #[arg(long, default_value = "0.1")]
    pub duration: f64,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Text,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown format: {other}")),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
        }
    }
}
