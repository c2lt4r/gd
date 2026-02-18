use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{ExtrudeArgs, OutputFormat, run_eval};

pub fn cmd_extrude(args: &ExtrudeArgs) -> Result<()> {
    let script = gdscript::generate_extrude(args.depth, args.segments);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let depth = parsed["depth"].as_f64().unwrap_or(0.0);
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            let fc = parsed["face_count"].as_u64().unwrap_or(0);
            println!(
                "Extruded: depth={}, vertices={vc}, faces={fc}",
                format!("{depth}").green().bold()
            );
        }
    }
    Ok(())
}
