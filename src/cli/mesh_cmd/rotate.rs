use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, RotateArgs, parse_3d, project_root, run_eval};
use crate::cprintln;

pub fn cmd_rotate(args: &RotateArgs) -> Result<()> {
    let (rx, ry, rz) = parse_3d(&args.degrees)?;
    let script = gdscript::generate_rotate(args.part.as_deref(), rx, ry, rz);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
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
