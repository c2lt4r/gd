use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::{MeshPart, MeshState};

use super::{OutputFormat, TranslateArgs, inject_stats, parse_3d, project_root, run_eval};
use gd_core::cprintln;

/// Compute the AABB center of a mesh part.
fn aabb_center(part: &MeshPart) -> [f64; 3] {
    let (amin, amax) = part.mesh.aabb();
    [
        (amin[0] + amax[0]) * 0.5,
        (amin[1] + amax[1]) * 0.5,
        (amin[2] + amax[2]) * 0.5,
    ]
}

/// Translate all vertices by a delta.
fn translate_verts(part: &mut MeshPart, delta: [f64; 3]) {
    for v in &mut part.mesh.vertices {
        v.position[0] += delta[0];
        v.position[1] += delta[1];
        v.position[2] += delta[2];
    }
}

pub fn cmd_translate(args: &TranslateArgs) -> Result<()> {
    if let Some(ref group_name) = args.group {
        return cmd_translate_group(args, group_name);
    }

    let (x, y, z) = parse_3d(&args.to)?;
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part_name = args.part.as_deref().unwrap_or(&state.active).to_string();

    // Bake translation into mesh vertices
    if args.relative {
        let part = state.resolve_part_mut(Some(&part_name))?;
        translate_verts(part, [x, y, z]);
    } else if let Some(ref ref_name) = args.relative_to {
        let ref_center = aabb_center(state.resolve_part(Some(ref_name))?);
        let target = [ref_center[0] + x, ref_center[1] + y, ref_center[2] + z];
        let part = state.resolve_part_mut(Some(&part_name))?;
        let center = aabb_center(part);
        translate_verts(
            part,
            [
                target[0] - center[0],
                target[1] - center[1],
                target[2] - center[2],
            ],
        );
    } else {
        let part = state.resolve_part_mut(Some(&part_name))?;
        let center = aabb_center(part);
        translate_verts(part, [x - center[0], y - center[1], z - center[2]]);
    }

    // Position is now baked — clear stored transform position
    let part = state.resolve_part_mut(Some(&part_name))?;
    part.transform.position = [0.0; 3];
    let new_center = aabb_center(part);
    state.save(&root)?;

    // Re-push to Godot (node transform will be identity)
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "name": part_name,
        "position": new_center,
    });
    if let Some(ref ref_name) = args.relative_to {
        result["relative_to"] = serde_json::json!(ref_name);
    }
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let name = result["name"].as_str().unwrap_or("?");
            if let Some(ref_name) = result["relative_to"].as_str() {
                cprintln!(
                    "Translated {} relative to {}: ({:.2}, {:.2}, {:.2})",
                    name.green().bold(),
                    ref_name.cyan(),
                    new_center[0],
                    new_center[1],
                    new_center[2],
                );
            } else {
                cprintln!(
                    "Translated {}: ({:.2}, {:.2}, {:.2})",
                    name.green().bold(),
                    new_center[0],
                    new_center[1],
                    new_center[2],
                );
            }
        }
    }
    Ok(())
}

fn cmd_translate_group(args: &TranslateArgs, group_name: &str) -> Result<()> {
    let (x, y, z) = parse_3d(&args.to)?;
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let members = state
        .groups
        .get(group_name)
        .ok_or_else(|| miette::miette!("Group '{group_name}' not found"))?
        .clone();

    for name in &members {
        let part = state
            .parts
            .get_mut(name)
            .ok_or_else(|| miette::miette!("Part '{name}' not found"))?;

        if args.relative {
            translate_verts(part, [x, y, z]);
        } else {
            let center = aabb_center(part);
            translate_verts(part, [x - center[0], y - center[1], z - center[2]]);
        }
        part.transform.position = [0.0; 3];
    }
    state.save(&root)?;

    for name in &members {
        let push = state.generate_push_script(name)?;
        let _ = run_eval(&push)?;
    }

    let mut result = serde_json::json!({
        "group": group_name,
        "members": members,
        "count": members.len(),
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Translated group {} ({} parts)",
                group_name.green().bold(),
                members.len().to_string().cyan()
            );
        }
    }
    Ok(())
}
