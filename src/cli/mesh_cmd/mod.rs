mod add_part;
mod bevel;
mod create;
mod describe;
mod duplicate_part;
mod extrude;
mod focus;
mod gdscript;
mod info;
mod init;
mod list_vertices;
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

use std::time::Duration;

use clap::{Args, Subcommand, ValueEnum};
use miette::{Result, miette};

use crate::core::live_eval::send_eval;
use crate::core::project::GodotProject;

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
    #[arg(long, default_value = "360")]
    pub angle: f64,
    /// Number of segments
    #[arg(long, default_value = "16")]
    pub segments: u32,
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
    /// Zoom multiplier (1.0 = auto-fit to model, 2.0 = zoom out 2x)
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
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
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
pub struct ScaleArgs {
    /// Part name (defaults to active part)
    #[arg(long)]
    pub part: Option<String>,
    /// Scale factor as "sx,sy,sz" or a single uniform value
    #[arg(long)]
    pub factor: String,
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
pub struct TaperArgs {
    /// Axis along which to taper (the extrusion depth axis)
    #[arg(long, value_enum)]
    pub axis: Axis,
    /// Scale factor at the start of the axis (1.0 = no change)
    #[arg(long, default_value = "1.0")]
    pub start: f64,
    /// Scale factor at the end of the axis (0.0 = taper to a point)
    #[arg(long)]
    pub end: f64,
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct BevelArgs {
    /// Bevel radius (offset distance from edge)
    #[arg(long)]
    pub radius: f64,
    /// Number of segments for the bevel curve
    #[arg(long, default_value = "2")]
    pub segments: u32,
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
    /// Zoom multiplier (1.0 = auto-fit to model)
    #[arg(long, default_value = "1.0")]
    pub zoom: f64,
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
fn run_eval(script: &str) -> Result<String> {
    let root = project_root()?;
    let result = send_eval(script, &root, MESH_TIMEOUT)?.result;
    if result.starts_with("ERROR:") {
        return Err(miette!("{result}"));
    }
    Ok(result)
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
