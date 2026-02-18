use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, TaperArgs, run_eval};

pub fn cmd_taper(args: &TaperArgs) -> Result<()> {
    let range = match (args.from, args.to) {
        (Some(f), Some(t)) => Some((f, t)),
        (Some(f), None) => Some((f, 1.0)),
        (None, Some(t)) => Some((0.0, t)),
        (None, None) => None,
    };
    let script = gdscript::generate_taper(
        args.part.as_deref(),
        args.axis.as_str(),
        args.start,
        args.end,
        args.midpoint,
        range,
    );
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let axis = parsed["axis"].as_str().unwrap_or("?");
            let start = parsed["start_scale"].as_f64().unwrap_or(0.0);
            let end = parsed["end_scale"].as_f64().unwrap_or(0.0);
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            println!(
                "Tapered along {}: {start:.2} -> {end:.2} ({vc} vertices)",
                axis.cyan()
            );
        }
    }
    Ok(())
}
