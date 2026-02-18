use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, RevolveArgs, run_eval};

pub fn cmd_revolve(args: &RevolveArgs) -> Result<()> {
    let script = gdscript::generate_revolve(args.axis.as_str(), args.degrees, args.segments);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let axis = parsed["axis"].as_str().unwrap_or("?");
            let angle = parsed["angle"].as_f64().unwrap_or(0.0);
            let segs = parsed["segments"].as_u64().unwrap_or(0);
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            println!(
                "Revolved: axis={}, angle={angle}, segments={segs}, vertices={vc}",
                axis.cyan()
            );
        }
    }
    Ok(())
}
