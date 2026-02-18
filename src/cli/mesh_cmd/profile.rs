use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, ProfileArgs, parse_points, run_eval};

pub fn cmd_profile(args: &ProfileArgs) -> Result<()> {
    let points = parse_points(&args.points)?;
    let script = gdscript::generate_profile(&points, args.plane.as_str());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let plane = parsed["plane"].as_str().unwrap_or("?");
            let count = parsed["point_count"].as_u64().unwrap_or(0);
            println!(
                "Profile set: {} points on {} plane",
                count.to_string().green().bold(),
                plane.cyan()
            );
        }
    }
    Ok(())
}
