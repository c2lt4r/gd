use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, RemovePartArgs, run_eval};

pub fn cmd_remove_part(args: &RemovePartArgs) -> Result<()> {
    let script = gdscript::generate_remove_part(&args.name);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let removed = parsed["removed"].as_str().unwrap_or("?");
            let active = parsed["active"].as_str().unwrap_or("none");
            let pc = parsed["part_count"].as_u64().unwrap_or(0);
            println!(
                "Removed: {} (active: {}, {pc} remaining)",
                removed.red().bold(),
                active.cyan(),
            );
        }
    }
    Ok(())
}
