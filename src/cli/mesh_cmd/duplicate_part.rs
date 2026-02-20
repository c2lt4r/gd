use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{DuplicatePartArgs, OutputFormat, project_root, run_eval};
use crate::cprintln;

pub fn cmd_duplicate_part(args: &DuplicatePartArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    // Clone the source part
    let src_part = state
        .parts
        .get(&args.name)
        .ok_or_else(|| miette!("Part '{}' not found", args.name))?
        .clone();

    let mut new_part = src_part;

    // Apply mirror if requested
    let mirror_axis = args.symmetric.as_ref().or(args.mirror.as_ref());
    if let Some(axis) = mirror_axis {
        crate::core::mesh::mirror::mirror(&mut new_part.mesh, axis.as_index());
        // Negate the position on the mirror axis so the duplicate appears
        // on the opposite side (e.g., --mirror x: X position gets negated)
        new_part.transform.position[axis.as_index()] =
            -new_part.transform.position[axis.as_index()];
    }

    state.parts.insert(args.as_name.clone(), new_part);
    state.active.clone_from(&args.as_name);
    state.save(&root)?;

    // Create the Godot node via GDScript
    let symmetric = args.symmetric.is_some();
    let script = if let Some(axis) = mirror_axis {
        gdscript::generate_mirror_part(&args.name, &args.as_name, axis.as_str(), symmetric)
    } else {
        gdscript::generate_duplicate_part(&args.name, &args.as_name)
    };
    let result = run_eval(&script)?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Push the new part's mesh
    let push = state.generate_push_script(&args.as_name)?;
    let _ = run_eval(&push)?;

    // Fix vertex_count: GDScript reports 0 because mesh is pushed after node creation.
    // Use the actual Rust-side vertex count.
    if let Some(part) = state.parts.get(&args.as_name) {
        parsed["vertex_count"] = serde_json::json!(part.mesh.vertices.len());
    }

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let src = &args.name;
            let dst = &args.as_name;
            let pc = state.parts.len();
            if let Some(axis) = mirror_axis {
                cprintln!(
                    "Mirrored {} -> {} (axis={}, {pc} parts total)",
                    src.cyan(),
                    dst.green().bold(),
                    axis.as_str().yellow(),
                );
            } else {
                cprintln!(
                    "Duplicated {} -> {} ({pc} parts total)",
                    src.cyan(),
                    dst.green().bold(),
                );
            }
        }
    }
    Ok(())
}
