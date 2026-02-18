use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, TranslateArgs, parse_3d, run_eval};

pub fn cmd_translate(args: &TranslateArgs) -> Result<()> {
    let (x, y, z) = parse_3d(&args.to)?;
    let script = gdscript::generate_translate(args.part.as_deref(), x, y, z, args.relative);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let new_pos = parsed["position"].as_array();
            if let Some(pos) = new_pos {
                println!(
                    "Translated {}: ({:.2}, {:.2}, {:.2})",
                    name.green().bold(),
                    pos[0].as_f64().unwrap_or(0.0),
                    pos[1].as_f64().unwrap_or(0.0),
                    pos[2].as_f64().unwrap_or(0.0),
                );
            }
        }
    }
    Ok(())
}
