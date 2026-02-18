use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{MaterialArgs, MaterialPreset, OutputFormat, run_eval};

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

pub fn cmd_material(args: &MaterialArgs) -> Result<()> {
    let color = args.color.as_ref().map(|c| normalize_color(c));

    // Batch mode: --parts pattern
    if let Some(ref pattern) = args.parts {
        let script = if let Some(ref preset) = args.preset {
            gdscript::generate_material_preset_multi(pattern, preset_name(preset), color.as_deref())
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

        match args.format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
            }
            OutputFormat::Text => {
                let count = parsed["count"].as_u64().unwrap_or(0);
                let pat = parsed["pattern"].as_str().unwrap_or("?");
                println!(
                    "Applied material to {} parts matching {}",
                    count.to_string().green(),
                    pat.cyan()
                );
            }
        }
        return Ok(());
    }

    // Single-part mode
    let script = if let Some(ref preset) = args.preset {
        gdscript::generate_material_preset(
            args.part.as_deref(),
            preset_name(preset),
            color.as_deref(),
        )
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

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            if let Some(preset) = parsed["preset"].as_str() {
                let metallic = parsed["metallic"].as_f64().unwrap_or(0.0);
                let roughness = parsed["roughness"].as_f64().unwrap_or(0.0);
                println!(
                    "Material {}: preset={}, metallic={metallic:.1}, roughness={roughness:.1}",
                    name.green().bold(),
                    preset.cyan(),
                );
            } else {
                let hex = parsed["color"].as_str().unwrap_or("?");
                println!("Material {}: color #{}", name.green().bold(), hex.cyan());
            }
        }
    }
    Ok(())
}
