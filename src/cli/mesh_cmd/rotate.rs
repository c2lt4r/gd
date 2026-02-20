use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, RotateArgs, inject_stats, parse_3d, project_root, run_eval};
use crate::cprintln;

pub fn cmd_rotate(args: &RotateArgs) -> Result<()> {
    if let Some(ref group_name) = args.group {
        return cmd_rotate_group(args, group_name);
    }

    let (rx, ry, rz) = parse_3d(&args.degrees)?;
    let script = gdscript::generate_rotate(args.part.as_deref(), rx, ry, rz);
    let result = run_eval(&script)?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Update Rust-side transform from Godot result
    if let Some(rot) = parsed["rotation"].as_array() {
        let root = project_root()?;
        if let Ok(mut state) = MeshState::load(&root) {
            let part_name = parsed["name"].as_str().unwrap_or(&state.active).to_string();
            if let Ok(part) = state.resolve_part_mut(Some(&part_name)) {
                part.transform.rotation = [
                    rot[0].as_f64().unwrap_or(0.0),
                    rot[1].as_f64().unwrap_or(0.0),
                    rot[2].as_f64().unwrap_or(0.0),
                ];
            }
            let _ = state.save(&root);
            inject_stats(&mut parsed, &state);
        }
    }

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            cprintln!(
                "Rotated {}: ({rx:.1}, {ry:.1}, {rz:.1}) degrees",
                name.green().bold(),
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

    for name in &members {
        let script = gdscript::generate_rotate(Some(name.as_str()), rx, ry, rz);
        let result = run_eval(&script)?;
        let parsed: serde_json::Value = serde_json::from_str(&result)
            .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

        if let Some(rot) = parsed["rotation"].as_array()
            && let Some(part) = state.parts.get_mut(name)
        {
            part.transform.rotation = [
                rot[0].as_f64().unwrap_or(0.0),
                rot[1].as_f64().unwrap_or(0.0),
                rot[2].as_f64().unwrap_or(0.0),
            ];
        }
    }
    let _ = state.save(&root);

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
