use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, ScaleArgs, parse_scale, run_eval};

pub fn cmd_scale(args: &ScaleArgs) -> Result<()> {
    let (sx, sy, sz) = parse_scale(&args.factor)?;
    let script = gdscript::generate_scale(args.part.as_deref(), sx, sy, sz);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            println!(
                "Scaled {}: ({sx:.2}, {sy:.2}, {sz:.2})",
                name.green().bold(),
            );
        }
    }
    Ok(())
}
