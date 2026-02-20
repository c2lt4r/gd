use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, ScaleArgs, parse_scale, project_root, run_eval};
use crate::cprintln;

pub fn cmd_scale(args: &ScaleArgs) -> Result<()> {
    let (sx, sy, sz) = parse_scale(&args.factor)?;
    let script = gdscript::generate_scale(args.part.as_deref(), sx, sy, sz, args.remap);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
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
