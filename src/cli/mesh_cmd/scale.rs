use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::{MeshState, Transform3D};

use super::{OutputFormat, ScaleArgs, inject_stats, parse_scale, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_scale(args: &ScaleArgs) -> Result<()> {
    if let Some(ref group_name) = args.group {
        return cmd_scale_group(args, group_name);
    }

    let (sx, sy, sz) = parse_scale(&args.factor)?;
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part_name = args.part.as_deref().unwrap_or(&state.active).to_string();
    let part = state.resolve_part_mut(Some(&part_name))?;

    // Bake scale into mesh vertices
    let transform = Transform3D {
        scale: [sx, sy, sz],
        ..Transform3D::default()
    };
    for v in &mut part.mesh.vertices {
        v.position = transform.apply_point(v.position);
    }

    // Remap: recenter after scaling
    if args.remap {
        let (aabb_min, aabb_max) = part.mesh.aabb();
        let center = [
            (aabb_min[0] + aabb_max[0]) * 0.5,
            (aabb_min[1] + aabb_max[1]) * 0.5,
            (aabb_min[2] + aabb_max[2]) * 0.5,
        ];
        for v in &mut part.mesh.vertices {
            v.position[0] -= center[0];
            v.position[1] -= center[1];
            v.position[2] -= center[2];
        }
    }

    // Scale is now baked — reset stored scale to identity
    part.transform.scale = [1.0; 3];
    state.save(&root)?;

    // Re-push to Godot so the visual matches
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let part = state.resolve_part(Some(&part_name))?;
    let (aabb_min, aabb_max) = part.mesh.aabb();

    let mut result = serde_json::json!({
        "name": part_name,
        "scale": [sx, sy, sz],
        "remap": args.remap,
        "aabb_min": aabb_min,
        "aabb_max": aabb_max,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Scaled {}: ({sx:.2}, {sy:.2}, {sz:.2})",
                part_name.green().bold(),
            );
        }
    }
    Ok(())
}

fn cmd_scale_group(args: &ScaleArgs, group_name: &str) -> Result<()> {
    let (sx, sy, sz) = parse_scale(&args.factor)?;
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let members = state
        .groups
        .get(group_name)
        .ok_or_else(|| miette::miette!("Group '{group_name}' not found"))?
        .clone();

    let transform = Transform3D {
        scale: [sx, sy, sz],
        ..Transform3D::default()
    };

    for name in &members {
        let part = state
            .parts
            .get_mut(name)
            .ok_or_else(|| miette::miette!("Part '{name}' not found"))?;

        for v in &mut part.mesh.vertices {
            v.position = transform.apply_point(v.position);
        }

        if args.remap {
            let (aabb_min, aabb_max) = part.mesh.aabb();
            let center = [
                (aabb_min[0] + aabb_max[0]) * 0.5,
                (aabb_min[1] + aabb_max[1]) * 0.5,
                (aabb_min[2] + aabb_max[2]) * 0.5,
            ];
            for v in &mut part.mesh.vertices {
                v.position[0] -= center[0];
                v.position[1] -= center[1];
                v.position[2] -= center[2];
            }
        }

        part.transform.scale = [1.0; 3];
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
                "Scaled group {} ({} parts): ({sx:.2}, {sy:.2}, {sz:.2})",
                group_name.green().bold(),
                members.len().to_string().cyan()
            );
        }
    }
    Ok(())
}
