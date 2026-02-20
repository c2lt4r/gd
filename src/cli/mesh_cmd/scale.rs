use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, ScaleArgs, inject_stats, parse_scale, project_root, run_eval};
use crate::cprintln;

pub fn cmd_scale(args: &ScaleArgs) -> Result<()> {
    if let Some(ref group_name) = args.group {
        return cmd_scale_group(args, group_name);
    }

    let (sx, sy, sz) = parse_scale(&args.factor)?;
    let script = gdscript::generate_scale(args.part.as_deref(), sx, sy, sz, args.remap);
    let result = run_eval(&script)?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Update Rust-side transform from Godot result
    let root = project_root()?;
    if let Ok(mut state) = MeshState::load(&root) {
        let part_name = parsed["name"].as_str().unwrap_or(&state.active).to_string();
        if let Ok(part) = state.resolve_part_mut(Some(&part_name)) {
            if let Some(sc) = parsed["scale"].as_array() {
                part.transform.scale = [
                    sc[0].as_f64().unwrap_or(1.0),
                    sc[1].as_f64().unwrap_or(1.0),
                    sc[2].as_f64().unwrap_or(1.0),
                ];
            }
            if let Some(pos) = parsed["position"].as_array() {
                part.transform.position = [
                    pos[0].as_f64().unwrap_or(0.0),
                    pos[1].as_f64().unwrap_or(0.0),
                    pos[2].as_f64().unwrap_or(0.0),
                ];
            }
        }
        let _ = state.save(&root);
        inject_stats(&mut parsed, &state);
    }

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            cprintln!(
                "Scaled {}: ({sx:.2}, {sy:.2}, {sz:.2})",
                name.green().bold(),
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

    for name in &members {
        let script = gdscript::generate_scale(Some(name.as_str()), sx, sy, sz, args.remap);
        let result = run_eval(&script)?;
        let parsed: serde_json::Value = serde_json::from_str(&result)
            .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

        if let Some(part) = state.parts.get_mut(name) {
            if let Some(sc) = parsed["scale"].as_array() {
                part.transform.scale = [
                    sc[0].as_f64().unwrap_or(1.0),
                    sc[1].as_f64().unwrap_or(1.0),
                    sc[2].as_f64().unwrap_or(1.0),
                ];
            }
            if let Some(pos) = parsed["position"].as_array() {
                part.transform.position = [
                    pos[0].as_f64().unwrap_or(0.0),
                    pos[1].as_f64().unwrap_or(0.0),
                    pos[2].as_f64().unwrap_or(0.0),
                ];
            }
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
                "Scaled group {} ({} parts): ({sx:.2}, {sy:.2}, {sz:.2})",
                group_name.green().bold(),
                members.len().to_string().cyan()
            );
        }
    }
    Ok(())
}
