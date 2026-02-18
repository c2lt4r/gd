use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{MaterialArgs, OutputFormat, run_eval};

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

pub fn cmd_material(args: &MaterialArgs) -> Result<()> {
    let color = normalize_color(&args.color);
    let script = gdscript::generate_material(args.part.as_deref(), &color);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let hex = parsed["color"].as_str().unwrap_or("?");
            println!(
                "Material {}: color #{}",
                name.green().bold(),
                hex.cyan()
            );
        }
    }
    Ok(())
}
