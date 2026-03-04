use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::{ExtrudeArgs, OutputFormat, inject_stats, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_extrude(args: &ExtrudeArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let (profile, plane, holes) = {
        let part = state.active_part()?;
        let profile = part
            .profile_points
            .as_ref()
            .ok_or_else(|| miette!("No profile. Run 'gd mesh profile' first."))?
            .clone();
        let plane = part
            .profile_plane
            .ok_or_else(|| miette!("No profile plane set."))?;
        let holes = part.profile_holes.clone().unwrap_or_default();
        (profile, plane, holes)
    };

    let mesh = if holes.is_empty() {
        // Auto-detect cap inset: enable at 0.15 for circle/ellipse profiles (8+ vertices)
        // unless explicitly set by the user
        let inset = match args.cap_inset {
            Some(v) => v,
            None if profile.len() >= 8 => 0.15,
            None => 0.0,
        };

        gd_mesh::extrude::extrude_with_inset(&profile, plane, args.depth, args.segments, inset)
            .ok_or_else(|| miette!("Failed to extrude (invalid profile?)"))?
    } else {
        gd_mesh::extrude::extrude_with_holes(&profile, &holes, plane, args.depth, args.segments)
            .ok_or_else(|| miette!("Failed to extrude with holes (invalid profile?)"))?
    };

    let vc = mesh.vertex_count();
    let fc = mesh.face_count();

    let part = state.active_part_mut()?;
    part.mesh = mesh;
    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&state.active.clone())?;
    let _ = run_eval(&push)?;

    let half = args.depth / 2.0;
    let mut result = serde_json::json!({
        "depth": args.depth,
        "plane": plane.as_str(),
        "segments": args.segments,
        "hole_count": holes.len(),
        "depth_range": [-half, half],
        "vertex_count": vc,
        "face_count": fc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Extruded: depth={}, vertices={vc}, faces={fc}",
                format!("{}", args.depth).green().bold()
            );
        }
    }
    Ok(())
}
