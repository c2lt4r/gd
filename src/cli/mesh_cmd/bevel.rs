use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{BevelArgs, OutputFormat, run_eval};

pub fn cmd_bevel(args: &BevelArgs) -> Result<()> {
    let script = gdscript::generate_bevel(args.radius, args.segments);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let r = parsed["radius"].as_f64().unwrap_or(0.0);
            let segs = parsed["segments"].as_u64().unwrap_or(0);
            let edges = parsed["sharp_edges"].as_u64().unwrap_or(0);
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            println!(
                "Beveled: radius {r:.3}, {segs} segments, {} sharp edges, {vc} vertices",
                edges.to_string().cyan()
            );
        }
    }
    Ok(())
}
