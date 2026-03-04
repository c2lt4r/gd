use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::{OutputFormat, SubdivideArgs, inject_stats, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_subdivide(args: &SubdivideArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part_name = args.part.clone().unwrap_or_else(|| state.active.clone());

    let part = state.resolve_part_mut(args.part.as_deref())?;
    let result_mesh = gd_mesh::subdivide::subdivide(&part.mesh, args.iterations);

    let vc = result_mesh.vertex_count();
    let fc = result_mesh.face_count();
    part.mesh = result_mesh;

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let iters = args.iterations;
    let mut result = serde_json::json!({
        "name": part_name,
        "iterations": iters,
        "face_count": fc,
        "vertex_count": vc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Subdivided {} ({iters} iteration{}): {fc} faces, {vc} vertices",
                part_name.cyan(),
                if iters == 1 { "" } else { "s" }
            );
        }
    }
    Ok(())
}
