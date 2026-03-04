use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::{OutputFormat, RevolveArgs, inject_stats, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_revolve(args: &RevolveArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let (profile, plane) = {
        let part = state.active_part()?;
        let profile = part
            .profile_points
            .as_ref()
            .ok_or_else(|| miette!("No profile. Run 'gd mesh profile' first."))?
            .clone();
        let plane = part
            .profile_plane
            .ok_or_else(|| miette!("No profile plane set."))?;
        (profile, plane)
    };

    let axis_idx = args.axis.as_index();

    let mesh = gd_mesh::revolve::revolve(
        &profile,
        plane,
        axis_idx,
        args.degrees,
        args.segments,
        args.cap,
    )
    .ok_or_else(|| miette!("Failed to revolve (invalid profile?)"))?;

    let vc = mesh.vertex_count();
    let fc = mesh.face_count();

    let part = state.active_part_mut()?;
    part.mesh = mesh;
    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&state.active.clone())?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "axis": args.axis.as_str(),
        "angle": args.degrees,
        "segments": args.segments,
        "vertex_count": vc,
        "face_count": fc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let angle = args.degrees;
            let segs = args.segments;
            cprintln!(
                "Revolved: axis={}, angle={angle}, segments={segs}, vertices={vc}",
                args.axis.as_str().cyan()
            );
        }
    }
    Ok(())
}
