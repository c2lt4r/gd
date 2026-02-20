use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{BevelArgs, OutputFormat, project_root, run_eval};
use crate::cprintln;

pub fn cmd_bevel(args: &BevelArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part = state.active_part_mut()?;
    let beveled = crate::core::mesh::bevel::bevel_with_profile(
        &part.mesh,
        args.radius,
        args.segments,
        args.edges.as_str(),
        args.profile,
    );

    let vc = beveled.vertex_count();
    let fc = beveled.face_count();
    part.mesh = beveled;

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&state.active.clone())?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "radius": args.radius,
        "segments": args.segments,
        "edges": args.edges.as_str(),
        "vertex_count": vc,
        "face_count": fc,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let r = args.radius;
            let segs = args.segments;
            cprintln!(
                "Beveled: radius {r:.3}, {segs} segments, {} edges, {vc} vertices",
                args.edges.as_str().cyan()
            );
        }
    }
    Ok(())
}
