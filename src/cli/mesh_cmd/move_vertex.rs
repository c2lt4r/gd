use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{MoveVertexArgs, OutputFormat, inject_stats, parse_3d, project_root, run_eval};
use crate::cprintln;

pub fn cmd_move_vertex(args: &MoveVertexArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let (dx, dy, dz) = parse_3d(&args.delta)?;
    let idx = args.index as usize;

    let part = state.active_part_mut()?;
    if idx >= part.mesh.vertices.len() {
        return Err(miette!(
            "Vertex index {idx} out of range (mesh has {} vertices)",
            part.mesh.vertices.len()
        ));
    }

    part.mesh.vertices[idx].position[0] += dx;
    part.mesh.vertices[idx].position[1] += dy;
    part.mesh.vertices[idx].position[2] += dz;

    let new_pos = part.mesh.vertices[idx].position;

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&state.active.clone())?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "index": idx,
        "position": new_pos,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Moved vertex {}: delta=({dx}, {dy}, {dz})",
                idx.to_string().green().bold()
            );
        }
    }
    Ok(())
}
