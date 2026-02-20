use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{MaterialArgs, MaterialPreset, OutputFormat, project_root, run_eval};
use crate::cprintln;

/// Normalize a color string: strip leading '#', expand named colors to hex.
fn normalize_color(input: &str) -> String {
    let s = input.strip_prefix('#').unwrap_or(input);
    match s.to_lowercase().as_str() {
        "red" => "ff0000".to_string(),
        "green" => "00ff00".to_string(),
        "blue" => "0000ff".to_string(),
        "white" => "ffffff".to_string(),
        "black" => "000000".to_string(),
        "yellow" => "ffff00".to_string(),
        "cyan" => "00ffff".to_string(),
        "magenta" => "ff00ff".to_string(),
        "orange" => "ff8800".to_string(),
        "gray" | "grey" => "808080".to_string(),
        _ => s.to_string(),
    }
}

fn preset_name(preset: &MaterialPreset) -> &'static str {
    match preset {
        MaterialPreset::Glass => "glass",
        MaterialPreset::Metal => "metal",
        MaterialPreset::Rubber => "rubber",
        MaterialPreset::Chrome => "chrome",
        MaterialPreset::Paint => "paint",
        MaterialPreset::Wood => "wood",
        MaterialPreset::Matte => "matte",
        MaterialPreset::Plastic => "plastic",
    }
}

/// Parse RGB from JSON result [r, g, b] (0.0–1.0) to [f32; 3].
fn parse_rgb(parsed: &serde_json::Value) -> Option<[f32; 3]> {
    let arr = parsed["rgb"].as_array()?;
    if arr.len() >= 3 {
        Some([
            arr[0].as_f64()? as f32,
            arr[1].as_f64()? as f32,
            arr[2].as_f64()? as f32,
        ])
    } else {
        None
    }
}

/// Persist material preset and color to Rust-side MeshState.
fn persist_material(
    part_names: &[&str],
    preset: Option<&str>,
    color_rgb: Option<[f32; 3]>,
) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;
    for &name in part_names {
        if let Some(part) = state.parts.get_mut(name) {
            part.material_preset = preset.map(String::from);
            if let Some(rgb) = color_rgb {
                part.color = Some(rgb);
            }
        }
    }
    state.save(&root)?;
    Ok(())
}

pub fn cmd_material(args: &MaterialArgs) -> Result<()> {
    let color = args.color.as_ref().map(|c| normalize_color(c));

    // Batch mode: --parts pattern
    if let Some(ref pattern) = args.parts {
        let preset_str = args.preset.as_ref().map(|p| preset_name(p));
        let script = if let Some(name) = preset_str {
            gdscript::generate_material_preset_multi(pattern, name, color.as_deref())
        } else if let Some(ref hex) = color {
            gdscript::generate_material_multi(pattern, hex)
        } else {
            return Err(miette::miette!(
                "Provide --color or --preset (e.g. --preset glass, --color ff0000)"
            ));
        };
        let result = run_eval(&script)?;
        let parsed: serde_json::Value = serde_json::from_str(&result)
            .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

        // Persist preset to Rust state for applied parts
        if let Some(applied) = parsed["applied"].as_array() {
            let names: Vec<&str> = applied
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect();
            if !names.is_empty() {
                let _ = persist_material(&names, preset_str, None);
            }
        }

        match args.format {
            OutputFormat::Json => {
                cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
            }
            OutputFormat::Text => {
                let count = parsed["count"].as_u64().unwrap_or(0);
                let pat = parsed["pattern"].as_str().unwrap_or("?");
                cprintln!(
                    "Applied material to {} parts matching {}",
                    count.to_string().green(),
                    pat.cyan()
                );
            }
        }
        // Warn about unmatched part names
        if let Some(skipped) = parsed["skipped"].as_array() {
            let names: Vec<&str> = skipped
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect();
            if !names.is_empty() {
                eprintln!(
                    "{}: no parts matched: {}",
                    "warning".yellow().bold(),
                    names.join(", ").yellow()
                );
            }
        }
        return Ok(());
    }

    // Single-part mode
    let preset_str = args.preset.as_ref().map(|p| preset_name(p));
    let script = if let Some(name) = preset_str {
        gdscript::generate_material_preset(args.part.as_deref(), name, color.as_deref())
    } else if let Some(ref hex) = color {
        gdscript::generate_material(args.part.as_deref(), hex)
    } else {
        return Err(miette::miette!(
            "Provide --color or --preset (e.g. --preset glass, --color ff0000)"
        ));
    };

    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Persist preset + color to Rust state
    if let Some(name) = parsed["name"].as_str() {
        let rgb = parse_rgb(&parsed);
        let _ = persist_material(&[name], preset_str, rgb);
    }

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            if let Some(preset) = parsed["preset"].as_str() {
                let metallic = parsed["metallic"].as_f64().unwrap_or(0.0);
                let roughness = parsed["roughness"].as_f64().unwrap_or(0.0);
                cprintln!(
                    "Material {}: preset={}, metallic={metallic:.1}, roughness={roughness:.1}",
                    name.green().bold(),
                    preset.cyan(),
                );
            } else {
                let hex = parsed["color"].as_str().unwrap_or("?");
                cprintln!("Material {}: color #{}", name.green().bold(), hex.cyan());
            }
        }
    }
    Ok(())
}
