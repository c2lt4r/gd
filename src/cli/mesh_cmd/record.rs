//! Recording of mesh commands to JSONL for deterministic replay.
//!
//! Every state-modifying command is captured as a single JSON line in
//! `.gd-mesh/replay.jsonl`. Read-only commands (view, info, etc.) are skipped.

use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde_json::json;

use super::MeshCommand;

thread_local! {
    static SUPPRESS_RECORDING: RefCell<bool> = const { RefCell::new(false) };
}

/// Enable or disable recording suppression (used during replay).
pub fn set_suppress(suppress: bool) {
    SUPPRESS_RECORDING.with(|s| *s.borrow_mut() = suppress);
}

/// Returns true if the command modifies state and should be recorded.
fn should_record(cmd: &MeshCommand) -> bool {
    !matches!(
        cmd,
        MeshCommand::Init(_)
            | MeshCommand::View(_)
            | MeshCommand::Snapshot(_)
            | MeshCommand::Reference(_)
            | MeshCommand::Focus(_)
            | MeshCommand::ListVertices(_)
            | MeshCommand::Info(_)
            | MeshCommand::Describe(_)
            | MeshCommand::Check(_)
            | MeshCommand::Batch(_)
            | MeshCommand::Replay(_)
            | MeshCommand::Groups(_)
    )
}

/// Record a command to `.gd-mesh/replay.jsonl` if appropriate.
///
/// Best-effort: recording failures are silently ignored so they never
/// block the actual command execution.
pub fn maybe_record(cmd: &MeshCommand, root: &Path) {
    let suppressed = SUPPRESS_RECORDING.with(|s| *s.borrow());
    if suppressed || !should_record(cmd) {
        return;
    }

    let line = command_to_json(cmd);
    let dir = root.join(".gd-mesh");
    let path = dir.join("replay.jsonl");

    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}

/// Convert a `MeshCommand` variant to a flat JSON string.
#[expect(clippy::too_many_lines)]
pub fn command_to_json(cmd: &MeshCommand) -> String {
    let value = match cmd {
        MeshCommand::Create(a) => json!({
            "command": "create",
            "name": a.name,
            "from": a.from.as_str(),
        }),
        MeshCommand::Profile(a) => profile_json(a),
        MeshCommand::Extrude(a) => {
            let mut v = json!({
                "command": "extrude",
                "depth": a.depth,
                "segments": a.segments,
            });
            if let Some(ci) = a.cap_inset {
                v["cap_inset"] = json!(ci);
            }
            v
        }
        MeshCommand::Revolve(a) => {
            let mut v = json!({
                "command": "revolve",
                "axis": a.axis.as_str(),
                "degrees": a.degrees,
                "segments": a.segments,
            });
            if a.cap {
                v["cap"] = json!(true);
            }
            v
        }
        MeshCommand::MoveVertex(a) => json!({
            "command": "move-vertex",
            "index": a.index,
            "delta": a.delta,
        }),
        MeshCommand::AddPart(a) => json!({
            "command": "add-part",
            "name": a.name,
            "from": a.from.as_str(),
        }),
        MeshCommand::DuplicatePart(a) => duplicate_part_json(a),
        MeshCommand::Translate(a) => translate_json(a),
        MeshCommand::Rotate(a) => rotate_json(a),
        MeshCommand::Scale(a) => scale_json(a),
        MeshCommand::RemovePart(a) => {
            let mut v = json!({"command": "remove-part"});
            if let Some(ref name) = a.name {
                v["name"] = json!(name);
            }
            if let Some(ref group) = a.group {
                v["group"] = json!(group);
            }
            v
        }
        MeshCommand::Taper(a) => taper_json(a),
        MeshCommand::Bevel(a) => {
            let mut v = json!({
                "command": "bevel",
                "radius": a.radius,
                "segments": a.segments,
                "edges": a.edges.as_str(),
                "profile": a.profile,
            });
            if let Some(ref w) = a.where_expr {
                v["where"] = json!(w);
            }
            v
        }
        MeshCommand::Checkpoint(a) => {
            let mut v = json!({"command": "checkpoint"});
            if let Some(ref name) = a.name {
                v["name"] = json!(name);
            }
            v
        }
        MeshCommand::Restore(a) => {
            let mut v = json!({"command": "restore"});
            if let Some(ref name) = a.name {
                v["name"] = json!(name);
            }
            v
        }
        MeshCommand::FlipNormals(a) => flip_normals_json(a),
        MeshCommand::FixNormals(a) => {
            let mut v = json!({"command": "fix-normals"});
            if let Some(ref part) = a.part {
                v["part"] = json!(part);
            }
            if a.all {
                v["all"] = json!(true);
            }
            v
        }
        MeshCommand::Material(a) => material_json(a),
        MeshCommand::LoopCut(a) => {
            let mut v = json!({
                "command": "loop-cut",
                "axis": a.axis.as_str(),
                "at": a.at,
            });
            if let Some(ref part) = a.part {
                v["part"] = json!(part);
            }
            v
        }
        MeshCommand::Subdivide(a) => {
            let mut v = json!({
                "command": "subdivide",
                "iterations": a.iterations,
            });
            if let Some(ref part) = a.part {
                v["part"] = json!(part);
            }
            v
        }
        MeshCommand::Boolean(a) => {
            let mut v = json!({
                "command": "boolean",
                "mode": bool_op_str(&a.mode),
                "tool": a.tool,
            });
            if let Some(ref offset) = a.offset {
                v["offset"] = json!(offset);
            }
            if let Some(count) = a.count {
                v["count"] = json!(count);
            }
            if let Some(ref spacing) = a.spacing {
                v["spacing"] = json!(spacing);
            }
            v
        }
        MeshCommand::Inset(a) => {
            let mut v = json!({
                "command": "inset",
                "factor": a.factor,
            });
            if let Some(ref w) = a.where_expr {
                v["where"] = json!(w);
            }
            v
        }
        MeshCommand::ExtrudeFace(a) => json!({
            "command": "extrude-face",
            "depth": a.depth,
            "where": a.where_expr,
        }),
        MeshCommand::Solidify(a) => json!({
            "command": "solidify",
            "thickness": a.thickness,
        }),
        MeshCommand::MergeVerts(a) => {
            let mut v = json!({
                "command": "merge-verts",
                "distance": a.distance,
            });
            if a.all {
                v["all"] = json!(true);
            }
            v
        }
        MeshCommand::Array(a) => json!({
            "command": "array",
            "count": a.count,
            "offset": a.offset,
        }),
        MeshCommand::ShadeSmooth(a) => shading_json("shade-smooth", a),
        MeshCommand::ShadeFlat(a) => shading_json("shade-flat", a),
        MeshCommand::AutoSmooth(a) => {
            let mut v = json!({
                "command": "auto-smooth",
                "angle": a.angle,
            });
            if let Some(ref part) = a.part {
                v["part"] = json!(part);
            }
            if a.all {
                v["all"] = json!(true);
            }
            v
        }
        MeshCommand::Group(a) => json!({
            "command": "group",
            "name": a.name,
            "parts": a.parts,
        }),
        MeshCommand::Ungroup(a) => json!({
            "command": "ungroup",
            "name": a.name,
        }),
        // Read-only commands are filtered by should_record — unreachable here.
        MeshCommand::Init(_)
        | MeshCommand::View(_)
        | MeshCommand::Snapshot(_)
        | MeshCommand::Reference(_)
        | MeshCommand::Focus(_)
        | MeshCommand::ListVertices(_)
        | MeshCommand::Info(_)
        | MeshCommand::Describe(_)
        | MeshCommand::Check(_)
        | MeshCommand::Batch(_)
        | MeshCommand::Replay(_)
        | MeshCommand::Groups(_) => unreachable!(),
    };
    serde_json::to_string(&value).unwrap()
}

// ── Per-command JSON builders ────────────────────────────────────────

fn profile_json(a: &super::ProfileArgs) -> serde_json::Value {
    let mut v = json!({"command": "profile"});
    if let Some(ref plane) = a.plane {
        v["plane"] = json!(plane.as_str());
    }
    if let Some(ref points) = a.points {
        v["points"] = json!(points);
    }
    if let Some(ref shape) = a.shape {
        v["shape"] = json!(shape_str(shape));
    }
    if let Some(r) = a.radius {
        v["radius"] = json!(r);
    }
    if let Some(rx) = a.radius_x {
        v["radius_x"] = json!(rx);
    }
    if let Some(ry) = a.radius_y {
        v["radius_y"] = json!(ry);
    }
    if a.segments != 32 {
        v["segments"] = json!(a.segments);
    }
    if let Some(w) = a.width {
        v["width"] = json!(w);
    }
    if let Some(h) = a.height {
        v["height"] = json!(h);
    }
    if let Some(ref src) = a.copy_profile_from {
        v["copy_profile_from"] = json!(src);
    }
    if !a.hole.is_empty() {
        v["hole"] = json!(a.hole);
    }
    v
}

fn duplicate_part_json(a: &super::DuplicatePartArgs) -> serde_json::Value {
    let mut v = json!({
        "command": "duplicate-part",
        "as": a.as_name,
    });
    if let Some(ref name) = a.name {
        v["name"] = json!(name);
    }
    if let Some(ref axis) = a.mirror {
        v["mirror"] = json!(axis.as_str());
    }
    if let Some(ref axis) = a.symmetric {
        v["symmetric"] = json!(axis.as_str());
    }
    if let Some(ref group) = a.group {
        v["group"] = json!(group);
    }
    if let Some(ref r) = a.replace {
        v["replace"] = json!(r);
    }
    if let Some(ref w) = a.with {
        v["with"] = json!(w);
    }
    v
}

fn translate_json(a: &super::TranslateArgs) -> serde_json::Value {
    let mut v = json!({
        "command": "translate",
        "to": a.to,
    });
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if let Some(ref group) = a.group {
        v["group"] = json!(group);
    }
    if a.relative {
        v["relative"] = json!(true);
    }
    if let Some(ref rt) = a.relative_to {
        v["relative_to"] = json!(rt);
    }
    v
}

fn rotate_json(a: &super::RotateArgs) -> serde_json::Value {
    let mut v = json!({
        "command": "rotate",
        "degrees": a.degrees,
    });
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if let Some(ref group) = a.group {
        v["group"] = json!(group);
    }
    v
}

fn scale_json(a: &super::ScaleArgs) -> serde_json::Value {
    let mut v = json!({
        "command": "scale",
        "factor": a.factor,
    });
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if let Some(ref group) = a.group {
        v["group"] = json!(group);
    }
    if a.remap {
        v["remap"] = json!(true);
    }
    v
}

fn taper_json(a: &super::TaperArgs) -> serde_json::Value {
    let mut v = json!({
        "command": "taper",
        "axis": a.axis.as_str(),
        "from_scale": a.from_scale,
        "to_scale": a.to_scale,
    });
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if let Some(mid) = a.midpoint {
        v["midpoint"] = json!(mid);
    }
    if let Some(from) = a.from {
        v["from"] = json!(from);
    }
    if let Some(to) = a.to {
        v["to"] = json!(to);
    }
    v
}

fn flip_normals_json(a: &super::FlipNormalsArgs) -> serde_json::Value {
    let mut v = json!({"command": "flip-normals"});
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if let Some(ref parts) = a.parts {
        v["parts"] = json!(parts);
    }
    if a.all {
        v["all"] = json!(true);
    }
    if let Some(ref axis) = a.caps {
        v["caps"] = json!(axis.as_str());
    }
    if let Some(ref w) = a.where_expr {
        v["where"] = json!(w);
    }
    v
}

fn material_json(a: &super::MaterialArgs) -> serde_json::Value {
    let mut v = json!({"command": "material"});
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if let Some(ref parts) = a.parts {
        v["parts"] = json!(parts);
    }
    if let Some(ref group) = a.group {
        v["group"] = json!(group);
    }
    if let Some(ref color) = a.color {
        v["color"] = json!(color);
    }
    if let Some(ref preset) = a.preset {
        v["preset"] = json!(preset_str(preset));
    }
    v
}

fn shading_json(name: &str, a: &super::ShadingArgs) -> serde_json::Value {
    let mut v = json!({"command": name});
    if let Some(ref part) = a.part {
        v["part"] = json!(part);
    }
    if a.all {
        v["all"] = json!(true);
    }
    v
}

// ── Enum → string helpers ────────────────────────────────────────────

fn shape_str(s: &super::ProfileShape) -> &'static str {
    match s {
        super::ProfileShape::Circle => "circle",
        super::ProfileShape::Rectangle => "rectangle",
        super::ProfileShape::Ellipse => "ellipse",
    }
}

fn preset_str(p: &super::MaterialPreset) -> &'static str {
    match p {
        super::MaterialPreset::Glass => "glass",
        super::MaterialPreset::Metal => "metal",
        super::MaterialPreset::Rubber => "rubber",
        super::MaterialPreset::Chrome => "chrome",
        super::MaterialPreset::Paint => "paint",
        super::MaterialPreset::Wood => "wood",
        super::MaterialPreset::Matte => "matte",
        super::MaterialPreset::Plastic => "plastic",
    }
}

fn bool_op_str(op: &super::BooleanOp) -> &'static str {
    match op {
        super::BooleanOp::Subtract => "subtract",
        super::BooleanOp::Union => "union",
        super::BooleanOp::Intersect => "intersect",
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::mesh_cmd::{
        Axis, CreateArgs, ExtrudeArgs, OutputFormat, Primitive, TaperArgs,
    };

    #[test]
    fn record_extrude_command() {
        let cmd = MeshCommand::Extrude(ExtrudeArgs {
            depth: 5.0,
            segments: 3,
            cap_inset: None,
            format: OutputFormat::Json,
        });
        let json_str = command_to_json(&cmd);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["command"], "extrude");
        assert_eq!(parsed["depth"], 5.0);
        assert_eq!(parsed["segments"], 3);
        assert!(parsed.get("cap_inset").is_none());
    }

    #[test]
    fn record_extrude_with_cap_inset() {
        let cmd = MeshCommand::Extrude(ExtrudeArgs {
            depth: 10.0,
            segments: 1,
            cap_inset: Some(0.15),
            format: OutputFormat::Json,
        });
        let json_str = command_to_json(&cmd);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["cap_inset"], 0.15);
    }

    #[test]
    fn record_roundtrip() {
        let cmd = MeshCommand::Create(CreateArgs {
            from: Primitive::Empty,
            name: "body".into(),
            format: OutputFormat::Json,
        });
        let json_str = command_to_json(&cmd);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["command"], "create");
        assert_eq!(parsed["name"], "body");
        assert_eq!(parsed["from"], "empty");

        let cmd2 = MeshCommand::Taper(TaperArgs {
            part: None,
            axis: Axis::Z,
            from_scale: 0.1,
            to_scale: 1.0,
            midpoint: Some(0.5),
            from: None,
            to: Some(0.8),
            format: OutputFormat::Json,
        });
        let json2 = command_to_json(&cmd2);
        let parsed2: serde_json::Value = serde_json::from_str(&json2).unwrap();
        assert_eq!(parsed2["command"], "taper");
        assert_eq!(parsed2["axis"], "z");
        assert_eq!(parsed2["from_scale"], 0.1);
        assert_eq!(parsed2["to_scale"], 1.0);
        assert_eq!(parsed2["midpoint"], 0.5);
        assert!(parsed2.get("from").is_none());
        assert_eq!(parsed2["to"], 0.8);
    }

    #[test]
    fn should_record_filters_readonly() {
        use crate::cli::mesh_cmd::*;

        // Read-only commands should NOT be recorded
        assert!(!should_record(&MeshCommand::View(ViewArgs {
            view: ViewName::All,
            output: None,
            grid: false,
            zoom: 1.0,
            normals: false,
            focus: None,
            format: OutputFormat::Json,
        })));
        assert!(!should_record(&MeshCommand::Info(InfoArgs {
            all: false,
            format: OutputFormat::Json,
        })));

        // State-modifying commands SHOULD be recorded
        assert!(should_record(&MeshCommand::Create(CreateArgs {
            from: Primitive::Empty,
            name: "body".into(),
            format: OutputFormat::Json,
        })));
        assert!(should_record(&MeshCommand::Extrude(ExtrudeArgs {
            depth: 5.0,
            segments: 1,
            cap_inset: None,
            format: OutputFormat::Json,
        })));
    }

    #[test]
    fn suppress_flag_works() {
        assert!(!SUPPRESS_RECORDING.with(|s| *s.borrow()));
        set_suppress(true);
        assert!(SUPPRESS_RECORDING.with(|s| *s.borrow()));
        set_suppress(false);
        assert!(!SUPPRESS_RECORDING.with(|s| *s.borrow()));
    }
}
