use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, RotateArgs, parse_3d, run_eval};

pub fn cmd_rotate(args: &RotateArgs) -> Result<()> {
    let (rx, ry, rz) = parse_3d(&args.degrees)?;
    let script = gdscript::generate_rotate(args.part.as_deref(), rx, ry, rz);
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
                "Rotated {}: ({rx:.1}, {ry:.1}, {rz:.1}) degrees",
                name.green().bold(),
            );
        }
    }
    Ok(())
}
