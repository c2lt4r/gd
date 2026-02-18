use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{DuplicatePartArgs, OutputFormat, run_eval};

pub fn cmd_duplicate_part(args: &DuplicatePartArgs) -> Result<()> {
    let script = if let Some(ref axis) = args.mirror {
        gdscript::generate_mirror_part(&args.name, &args.as_name, axis.as_str())
    } else {
        gdscript::generate_duplicate_part(&args.name, &args.as_name)
    };
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let src = parsed["source"].as_str().unwrap_or("?");
            let dst = parsed["name"].as_str().unwrap_or("?");
            let pc = parsed["part_count"].as_u64().unwrap_or(0);
            if let Some(mirror) = parsed["mirror"].as_str() {
                println!(
                    "Mirrored {} -> {} (axis={}, {pc} parts total)",
                    src.cyan(),
                    dst.green().bold(),
                    mirror.yellow(),
                );
            } else {
                println!(
                    "Duplicated {} -> {} ({pc} parts total)",
                    src.cyan(),
                    dst.green().bold(),
                );
            }
        }
    }
    Ok(())
}
