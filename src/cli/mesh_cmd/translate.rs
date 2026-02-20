use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, TranslateArgs, parse_3d, project_root, run_eval};
use crate::cprintln;

pub fn cmd_translate(args: &TranslateArgs) -> Result<()> {
    let (x, y, z) = parse_3d(&args.to)?;
    let script = if let Some(ref ref_part) = args.relative_to {
        gdscript::generate_translate_relative_to(args.part.as_deref(), ref_part, x, y, z)
    } else {
        gdscript::generate_translate(args.part.as_deref(), x, y, z, args.relative)
    };
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Update Rust-side transform from Godot result
    if let Some(pos) = parsed["position"].as_array() {
        let root = project_root()?;
        if let Ok(mut state) = MeshState::load(&root) {
            let part_name = parsed["name"].as_str().unwrap_or(&state.active).to_string();
            if let Ok(part) = state.resolve_part_mut(Some(&part_name)) {
                part.transform.position = [
                    pos[0].as_f64().unwrap_or(0.0),
                    pos[1].as_f64().unwrap_or(0.0),
                    pos[2].as_f64().unwrap_or(0.0),
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
            let new_pos = parsed["position"].as_array();
            if let Some(pos) = new_pos {
                if let Some(ref_name) = parsed["relative_to"].as_str() {
                    cprintln!(
                        "Translated {} relative to {}: ({:.2}, {:.2}, {:.2})",
                        name.green().bold(),
                        ref_name.cyan(),
                        pos[0].as_f64().unwrap_or(0.0),
                        pos[1].as_f64().unwrap_or(0.0),
                        pos[2].as_f64().unwrap_or(0.0),
                    );
                } else {
                    cprintln!(
                        "Translated {}: ({:.2}, {:.2}, {:.2})",
                        name.green().bold(),
                        pos[0].as_f64().unwrap_or(0.0),
                        pos[1].as_f64().unwrap_or(0.0),
                        pos[2].as_f64().unwrap_or(0.0),
                    );
                }
            }
        }
    }
    Ok(())
}
