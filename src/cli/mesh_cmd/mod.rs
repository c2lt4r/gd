mod add_part;
mod bevel;
mod checkpoint;
mod create;
mod describe;
mod duplicate_part;
mod extrude;
mod fix_normals;
mod flip_normals;
mod focus;
mod gdscript;
mod info;
mod init;
mod list_vertices;
mod loop_cut;
mod material;
mod move_vertex;
mod profile;
mod reference;
mod remove_part;
mod revolve;
mod rotate;
mod scale;
mod snapshot;
mod taper;
mod translate;
mod view;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::time::Duration;

use clap::{Args, Subcommand, ValueEnum};
use miette::{Result, miette};

use crate::core::live_eval::send_eval;
use crate::core::project::GodotProject;

thread_local! {
    static CURRENT_COMMAND: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Default timeout for mesh eval commands.
const MESH_TIMEOUT: Duration = Duration::from_secs(10);

/// Bounding box defined by two corner points.
type BoundingBox = ((f64, f64, f64), (f64, f64, f64));

#[derive(Args)]
pub struct MeshArgs {
    #[command(subcommand)]
    pub command: MeshCommand,
}

#[derive(Subcommand)]
pub enum MeshCommand {
    /// Create a 3D workspace scene with lighting and environment
    Init(InitArgs),
    /// Bootstrap a mesh editing session with camera rig and optional primitive
    Create(CreateArgs),
    /// Define a 2D profile polygon on a plane
    Profile(ProfileArgs),
    /// Extrude the current profile into 3D
    Extrude(ExtrudeArgs),
    /// Revolve the current profile around an axis
    Revolve(RevolveArgs),
    /// Move a single vertex by a delta offset
    #[command(name = "move-vertex")]
    MoveVertex(MoveVertexArgs),
    /// Take orthographic screenshots for AI feedback
    View(ViewArgs),
    /// Export the current mesh to a .tscn scene file
    Snapshot(SnapshotArgs),
    /// Validate and return a reference image path
    Reference(ReferenceArgs),
    /// Add a named sub-part to the current mesh session
    #[command(name = "add-part")]
    AddPart(AddPartArgs),
    /// Clone an existing part's mesh and transform to a new name
    #[command(name = "duplicate-part")]
    DuplicatePart(DuplicatePartArgs),
    /// Switch active part (or show all parts)
    Focus(FocusArgs),
    /// Move a part to an absolute position or by a relative offset
    Translate(TranslateArgs),
    /// Rotate a part by euler angles
    Rotate(RotateArgs),
    /// Scale a part by a factor per axis
    Scale(ScaleArgs),
    /// Remove a part from the session
    #[command(name = "remove-part")]
    RemovePart(RemovePartArgs),
    /// List vertex positions of the active mesh
    #[command(name = "list-vertices")]
    ListVertices(ListVerticesArgs),
    /// Taper an extruded mesh along its depth axis
    Taper(TaperArgs),
    /// Bevel (chamfer) edges of the active mesh
    Bevel(BevelArgs),
    /// Show current mesh session info (vertices, AABB, profile state)
    Info(InfoArgs),
    /// One-shot session debrief: part inventory + composite screenshots
    Describe(DescribeArgs),
    /// Save a checkpoint of all part meshes for later restore
    Checkpoint(CheckpointArgs),
    /// Restore all part meshes from the last checkpoint
    Restore(RestoreArgs),
    /// Flip triangle winding to fix inverted normals
    #[command(name = "flip-normals")]
    FlipNormals(FlipNormalsArgs),
    /// Auto-detect and fix inverted normals (recalculate outward)
    #[command(name = "fix-normals")]
    FixNormals(FixNormalsArgs),
    /// Set material color on a part
    Material(MaterialArgs),
    /// Subdivide mesh by inserting an axis-aligned cut plane
    #[command(name = "loop-cut")]
    LoopCut(LoopCutArgs),
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Plane {
    Front,
    Side,
    Top,
}

impl Plane {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Side => "side",
            Self::Top => "top",
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Primitive {
    Cube,
    Sphere,
    Cylinder,
    Empty,
}

impl Primitive {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Cube => "cube",
            Self::Sphere => "sphere",
            Self::Cylinder => "cylinder",
            Self::Empty => "empty",
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ViewName {
    Front,
    Back,
    Side,
    Left,
    Top,
    Bottom,
    Iso,
    All,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn as_str(&self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }
    }
}

#[derive(Args)]
pub struct InitArgs {
    /// Scene file name
    #[arg(long, default_value = "_mesh_workspace.tscn")]
    pub scene: String,
    /// Overwrite existing scene
    #[arg(long)]
    pub force: bool,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Starting primitive mesh
    #[arg(long, value_enum, default_value = "empty")]
    pub from: Primitive,
    /// Name for the mesh node
    #[arg(long, default_value = "GdMesh")]
    pub name: String,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ProfileArgs {
    /// Plane to draw profile on
    #[arg(long, value_enum)]
    pub plane: Plane,
    /// 2D points as "x1,y1 x2,y2 x3,y3 ..."
    #[arg(long, allow_hyphen_values = true)]
    pub points: String,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ExtrudeArgs {
    /// Extrusion depth
    #[arg(long)]
    pub depth: f64,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct RevolveArgs {
    /// Axis to revolve around
    #[arg(long, value_enum)]
    pub axis: Axis,
    /// Angle in degrees
    #[arg(long, alias = "angle", default_value = "360")]
    pub degrees: f64,
    /// Number of segments
    #[arg(long, default_value = "16")]
    pub segments: u32,
    /// Cap open ends of partial revolves (angle < 360)
    #[arg(long)]
    pub cap: bool,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct MoveVertexArgs {
    /// Vertex index
    pub index: u32,
    /// Delta as "dx,dy,dz"
    #[arg(long, allow_hyphen_values = true)]
    pub delta: String,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ViewArgs {
    /// View to capture (front, side, top, iso, or all)
    #[arg(value_enum, default_value = "all")]
    pub view: ViewName,
    /// Output directory for screenshots
    #[arg(short, long)]
    pub output: Option<String>,
    /// Show coordinate grid overlay
    #[arg(long)]
    pub grid: bool,
    /// Zoom level (1.0 = auto-fit, 2.0 = 2x closer, 0.5 = 2x farther)
    #[arg(long, default_value = "1.0")]
    pub zoom: f64,
    /// Show face orientation overlay (blue = front-facing, red = back-facing)
    #[arg(long)]
    pub normals: bool,
    /// Focus a specific part or "all" parts before capturing
    #[arg(long)]
    pub focus: Option<String>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SnapshotArgs {
    /// Path for the .tscn file (relative to project root)
    pub path: String,
    /// Preview without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ReferenceArgs {
    /// Path to reference image
    #[arg(long)]
    pub path: String,
    /// Which view this reference corresponds to
    #[arg(long, value_enum)]
    pub view: Option<Plane>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct AddPartArgs {
    /// Name for the new part
    #[arg(long)]
    pub name: String,
    /// Starting primitive mesh
    #[arg(long, value_enum, default_value = "empty")]
    pub from: Primitive,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct DuplicatePartArgs {
    /// Source part to clone
    #[arg(long)]
    pub name: String,
    /// Name for the new copy
    #[arg(long = "as")]
    pub as_name: String,
    /// Mirror across an axis (flips mesh vertices and fixes normals)
    #[arg(long, value_enum)]
    pub mirror: Option<Axis>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct FocusArgs {
    /// Part name to focus on
    pub part: Option<String>,
    /// Show all parts at once
    #[arg(long)]
    pub all: bool,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
#[command(allow_hyphen_values = true)]
pub struct TranslateArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Position or offset as "x,y,z"
    #[arg(long, allow_hyphen_values = true)]
    pub to: String,
    /// Treat --to as relative offset instead of absolute position
    #[arg(long)]
    pub relative: bool,
    /// Position relative to another part's AABB center (offset added to that center)
    #[arg(long)]
    pub relative_to: Option<String>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
#[command(allow_hyphen_values = true)]
pub struct RotateArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Rotation in degrees as "rx,ry,rz"
    #[arg(long, allow_hyphen_values = true)]
    pub degrees: String,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
#[command(allow_hyphen_values = true)]
pub struct ScaleArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Scale factor as "sx,sy,sz" or a single uniform value
    #[arg(long)]
    pub factor: String,
    /// Re-center after scaling (keeps AABB center at the same position)
    #[arg(long)]
    pub remap: bool,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct RemovePartArgs {
    /// Name of the part to remove
    #[arg(long)]
    pub name: String,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct ListVerticesArgs {
    /// Filter to bounding box as "x1,y1,z1 x2,y2,z2"
    #[arg(long, allow_hyphen_values = true)]
    pub region: Option<String>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
#[command(allow_hyphen_values = true)]
pub struct TaperArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Axis along which to taper (the extrusion depth axis)
    #[arg(long, value_enum)]
    pub axis: Axis,
    /// Scale factor at the start of the axis (1.0 = no change)
    #[arg(long, default_value = "1.0")]
    pub start: f64,
    /// Scale factor at the end of the axis (0.0 = taper to a point)
    #[arg(long)]
    pub end: f64,
    /// Peak position along axis (0.0-1.0) for two-segment taper (fat middle, thin ends)
    #[arg(long)]
    pub midpoint: Option<f64>,
    /// Start of taper range as normalized axis position (0.0-1.0, default 0.0)
    #[arg(long)]
    pub from: Option<f64>,
    /// End of taper range as normalized axis position (0.0-1.0, default 1.0)
    #[arg(long)]
    pub to: Option<f64>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum BevelEdges {
    All,
    Depth,
    Profile,
}

impl BevelEdges {
    fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Depth => "depth",
            Self::Profile => "profile",
        }
    }
}

#[derive(Args)]
pub struct BevelArgs {
    /// Bevel radius (offset distance from edge)
    #[arg(long)]
    pub radius: f64,
    /// Number of segments for the bevel curve
    #[arg(long, default_value = "2")]
    pub segments: u32,
    /// Which edges to bevel (all, depth=extrusion-direction, profile=cap-outline)
    #[arg(long, value_enum, default_value = "all")]
    pub edges: BevelEdges,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct InfoArgs {
    /// Show summary of all parts instead of just active
    #[arg(long)]
    pub all: bool,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct DescribeArgs {
    /// View to capture (front, side, top, iso, or all)
    #[arg(value_enum, default_value = "all")]
    pub view: ViewName,
    /// Output directory for screenshots
    #[arg(short, long)]
    pub output: Option<String>,
    /// Zoom level (1.0 = auto-fit, 2.0 = 2x closer, 0.5 = 2x farther)
    #[arg(long, default_value = "1.0")]
    pub zoom: f64,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct CheckpointArgs {
    /// Named checkpoint label (omit for default unnamed checkpoint)
    #[arg(long)]
    pub name: Option<String>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct RestoreArgs {
    /// Named checkpoint to restore (omit for default unnamed checkpoint)
    #[arg(long)]
    pub name: Option<String>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct FlipNormalsArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Only flip faces whose normal aligns with this axis (cap faces from extrude/revolve)
    #[arg(long, value_enum)]
    pub caps: Option<Axis>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct FixNormalsArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct MaterialArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Color as hex (e.g. "ff0000" or "#ff0000") or named color (red, green, blue, white, black)
    #[arg(long)]
    pub color: Option<String>,
    /// PBR material preset (glass, metal, rubber, chrome, paint, wood, matte, plastic)
    #[arg(long, value_enum)]
    pub preset: Option<MaterialPreset>,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum MaterialPreset {
    Glass,
    Metal,
    Rubber,
    Chrome,
    Paint,
    Wood,
    Matte,
    Plastic,
}

#[derive(Args)]
#[command(allow_hyphen_values = true)]
pub struct LoopCutArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Axis perpendicular to the cut plane
    #[arg(long, value_enum)]
    pub axis: Axis,
    /// Position along the axis to cut (world-space coordinate)
    #[arg(long)]
    pub at: f64,
    /// Output format
    #[arg(long, default_value = "json")]
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

pub fn exec(args: &MeshArgs) -> Result<()> {
    // Set the command name for HUD display
    let cmd_name = format!("gd mesh {}", command_name(&args.command));
    CURRENT_COMMAND.with(|c| *c.borrow_mut() = cmd_name);

    match args.command {
        MeshCommand::Init(ref a) => init::cmd_init(a),
        MeshCommand::Create(ref a) => create::cmd_create(a),
        MeshCommand::Profile(ref a) => profile::cmd_profile(a),
        MeshCommand::Extrude(ref a) => extrude::cmd_extrude(a),
        MeshCommand::Revolve(ref a) => revolve::cmd_revolve(a),
        MeshCommand::MoveVertex(ref a) => move_vertex::cmd_move_vertex(a),
        MeshCommand::View(ref a) => view::cmd_view(a),
        MeshCommand::Snapshot(ref a) => snapshot::cmd_snapshot(a),
        MeshCommand::Reference(ref a) => reference::cmd_reference(a),
        MeshCommand::AddPart(ref a) => add_part::cmd_add_part(a),
        MeshCommand::DuplicatePart(ref a) => duplicate_part::cmd_duplicate_part(a),
        MeshCommand::Focus(ref a) => focus::cmd_focus(a),
        MeshCommand::Translate(ref a) => translate::cmd_translate(a),
        MeshCommand::Rotate(ref a) => rotate::cmd_rotate(a),
        MeshCommand::Scale(ref a) => scale::cmd_scale(a),
        MeshCommand::RemovePart(ref a) => remove_part::cmd_remove_part(a),
        MeshCommand::ListVertices(ref a) => list_vertices::cmd_list_vertices(a),
        MeshCommand::Taper(ref a) => taper::cmd_taper(a),
        MeshCommand::Bevel(ref a) => bevel::cmd_bevel(a),
        MeshCommand::Info(ref a) => info::cmd_info(a),
        MeshCommand::Describe(ref a) => describe::cmd_describe(a),
        MeshCommand::Checkpoint(ref a) => checkpoint::cmd_checkpoint(a),
        MeshCommand::Restore(ref a) => checkpoint::cmd_restore(a),
        MeshCommand::FlipNormals(ref a) => flip_normals::cmd_flip_normals(a),
        MeshCommand::FixNormals(ref a) => fix_normals::cmd_fix_normals(a),
        MeshCommand::Material(ref a) => material::cmd_material(a),
        MeshCommand::LoopCut(ref a) => loop_cut::cmd_loop_cut(a),
    }
}

/// Map a `MeshCommand` variant to its CLI subcommand name for HUD display.
fn command_name(cmd: &MeshCommand) -> &'static str {
    match cmd {
        MeshCommand::Init(_) => "init",
        MeshCommand::Create(_) => "create",
        MeshCommand::Profile(_) => "profile",
        MeshCommand::Extrude(_) => "extrude",
        MeshCommand::Revolve(_) => "revolve",
        MeshCommand::MoveVertex(_) => "move-vertex",
        MeshCommand::View(_) => "view",
        MeshCommand::Snapshot(_) => "snapshot",
        MeshCommand::Reference(_) => "reference",
        MeshCommand::AddPart(_) => "add-part",
        MeshCommand::DuplicatePart(_) => "duplicate-part",
        MeshCommand::Focus(_) => "focus",
        MeshCommand::Translate(_) => "translate",
        MeshCommand::Rotate(_) => "rotate",
        MeshCommand::Scale(_) => "scale",
        MeshCommand::RemovePart(_) => "remove-part",
        MeshCommand::ListVertices(_) => "list-vertices",
        MeshCommand::Taper(_) => "taper",
        MeshCommand::Bevel(_) => "bevel",
        MeshCommand::Info(_) => "info",
        MeshCommand::Describe(_) => "describe",
        MeshCommand::Checkpoint(_) => "checkpoint",
        MeshCommand::Restore(_) => "restore",
        MeshCommand::FlipNormals(_) => "flip-normals",
        MeshCommand::FixNormals(_) => "fix-normals",
        MeshCommand::Material(_) => "material",
        MeshCommand::LoopCut(_) => "loop-cut",
    }
}

// ── Shared helpers ───────────────────────────────────────────────────

/// Resolve the project root.
fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project = GodotProject::discover(&cwd)?;
    Ok(project.root)
}

/// Run a generated GDScript via live eval and return the raw result string.
///
/// Injects a HUD overlay update at the start of `func run():` so the human
/// can see which command the agent is executing in the Godot viewport.
fn run_eval(script: &str) -> Result<String> {
    run_eval_hud(script, None)
}

/// Like `run_eval` but with an explicit HUD label override (for internal scripts
/// like camera switch where the auto-detected label would be confusing).
fn run_eval_hud(script: &str, hud_label: Option<&str>) -> Result<String> {
    let root = project_root()?;

    // Inject HUD update into the script's run() function
    let injected = inject_hud(script, hud_label);
    let result = send_eval(&injected, &root, MESH_TIMEOUT)?.result;
    if result.starts_with("ERROR:") {
        return Err(miette!("{result}"));
    }
    Ok(result)
}

/// Inject HUD overlay update code after `func run():` in a generated GDScript.
/// If the script doesn't contain `func run():`, returns it unchanged.
fn inject_hud(script: &str, label_override: Option<&str>) -> String {
    let label = label_override.map_or_else(
        || CURRENT_COMMAND.with(|c| c.borrow().clone()),
        String::from,
    );

    // Find "func run():\n" and inject HUD update lines after it
    let marker = "func run():\n";
    if let Some(pos) = script.find(marker) {
        let insert_at = pos + marker.len();
        let hud_code = format!(
            "\tvar _hud_helper = get_tree().get_root().get_node_or_null(\"_GdMeshHelper\")\n\
             \tif _hud_helper:\n\
             \t\tvar _hud = _hud_helper.get_node_or_null(\"_HudLayer/_HudLabel\")\n\
             \t\tif _hud: _hud.text = \"{label}\"\n"
        );
        let mut result = String::with_capacity(script.len() + hud_code.len());
        result.push_str(&script[..insert_at]);
        result.push_str(&hud_code);
        result.push_str(&script[insert_at..]);
        result
    } else {
        script.to_string()
    }
}

/// Parse "x1,y1 x2,y2 ..." into a Vec of (f64, f64) pairs.
fn parse_points(s: &str) -> Result<Vec<(f64, f64)>> {
    let mut points = Vec::new();
    for pair in s.split_whitespace() {
        let parts: Vec<&str> = pair.split(',').collect();
        if parts.len() != 2 {
            return Err(miette!("Invalid point '{pair}' — expected x,y"));
        }
        let x: f64 = parts[0]
            .parse()
            .map_err(|_| miette!("Invalid x in '{pair}'"))?;
        let y: f64 = parts[1]
            .parse()
            .map_err(|_| miette!("Invalid y in '{pair}'"))?;
        points.push((x, y));
    }
    if points.len() < 3 {
        return Err(miette!(
            "Need at least 3 points for a polygon, got {}",
            points.len()
        ));
    }
    Ok(points)
}

/// Parse a scale factor — either a single uniform value or "sx,sy,sz".
fn parse_scale(s: &str) -> Result<(f64, f64, f64)> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 1 {
        let v: f64 = parts[0]
            .trim()
            .parse()
            .map_err(|_| miette!("Invalid scale: {}", parts[0]))?;
        return Ok((v, v, v));
    }
    parse_3d(s)
}

/// Parse "dx,dy,dz" into a tuple.
fn parse_3d(s: &str) -> Result<(f64, f64, f64)> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err(miette!("Invalid 3D value '{s}' — expected dx,dy,dz"));
    }
    let x: f64 = parts[0]
        .trim()
        .parse()
        .map_err(|_| miette!("Invalid dx: {}", parts[0]))?;
    let y: f64 = parts[1]
        .trim()
        .parse()
        .map_err(|_| miette!("Invalid dy: {}", parts[1]))?;
    let z: f64 = parts[2]
        .trim()
        .parse()
        .map_err(|_| miette!("Invalid dz: {}", parts[2]))?;
    Ok((x, y, z))
}
