use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{LoopCutArgs, OutputFormat, run_eval};

pub fn cmd_loop_cut(args: &LoopCutArgs) -> Result<()> {
    let script = gdscript::generate_loop_cut(args.part.as_deref(), args.axis.as_str(), args.at);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let axis = parsed["axis"].as_str().unwrap_or("?");
            let at = parsed["at"].as_f64().unwrap_or(0.0);
            let splits = parsed["triangles_split"].as_u64().unwrap_or(0);
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            println!(
                "Loop cut at {}={at:.2}: {splits} triangles split, {vc} vertices",
                axis.cyan()
            );
        }
    }
    Ok(())
}
