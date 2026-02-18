use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, ProfileArgs, parse_points, run_eval};

pub fn cmd_profile(args: &ProfileArgs) -> Result<()> {
    let script = if let Some(ref src_part) = args.copy_profile_from {
        gdscript::generate_profile_from_part(src_part)
    } else {
        let points_str = args
            .points
            .as_deref()
            .ok_or_else(|| miette!("--points is required when not using --copy-profile-from"))?;
        let plane = args
            .plane
            .as_ref()
            .ok_or_else(|| miette!("--plane is required when not using --copy-profile-from"))?;
        let points = parse_points(points_str)?;
        gdscript::generate_profile(&points, plane.as_str())
    };
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let plane = parsed["plane"].as_str().unwrap_or("?");
            let count = parsed["point_count"].as_u64().unwrap_or(0);
            if let Some(src) = parsed["copied_from"].as_str() {
                println!(
                    "Profile copied from {}: {} points on {} plane",
                    src.cyan(),
                    count.to_string().green().bold(),
                    plane.cyan()
                );
            } else {
                println!(
                    "Profile set: {} points on {} plane",
                    count.to_string().green().bold(),
                    plane.cyan()
                );
            }
        }
    }
    Ok(())
}
