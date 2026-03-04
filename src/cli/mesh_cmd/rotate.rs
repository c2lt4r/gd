use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::{MeshState, Transform3D};

use super::{OutputFormat, RotateArgs, inject_stats, parse_3d, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_rotate(args: &RotateArgs) -> Result<()> {
    if let Some(ref group_name) = args.group {
        return cmd_rotate_group(args, group_name);
    }

    let (rx, ry, rz) = parse_3d(&args.degrees)?;
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part_name = args.part.as_deref().unwrap_or(&state.active).to_string();
    let part = state.resolve_part_mut(Some(&part_name))?;

    // Bake rotation into mesh vertices
    let transform = Transform3D {
        rotation: [rx, ry, rz],
        ..Transform3D::default()
    };
    for v in &mut part.mesh.vertices {
        v.position = transform.apply_point(v.position);
    }

    // Rotation is now baked — reset stored rotation to identity
    part.transform.rotation = [0.0; 3];
    state.save(&root)?;

    // Re-push to Godot
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "name": part_name,
        "rotation": [rx, ry, rz],
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Rotated {}: ({rx:.1}, {ry:.1}, {rz:.1}) degrees",
                part_name.green().bold(),
            );
        }
    }
    Ok(())
}

fn cmd_rotate_group(args: &RotateArgs, group_name: &str) -> Result<()> {
    let (rx, ry, rz) = parse_3d(&args.degrees)?;
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let members = state
        .groups
        .get(group_name)
        .ok_or_else(|| miette::miette!("Group '{group_name}' not found"))?
        .clone();

    let transform = Transform3D {
        rotation: [rx, ry, rz],
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
        part.transform.rotation = [0.0; 3];
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
                "Rotated group {} ({} parts): ({rx:.1}, {ry:.1}, {rz:.1}) degrees",
                group_name.green().bold(),
                members.len().to_string().cyan()
            );
        }
    }
    Ok(())
}
