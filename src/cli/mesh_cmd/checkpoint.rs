use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{CheckpointArgs, OutputFormat, RestoreArgs, run_eval};

pub fn cmd_checkpoint(args: &CheckpointArgs) -> Result<()> {
    let script = gdscript::generate_checkpoint(args.name.as_deref());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let count = parsed["parts_saved"].as_u64().unwrap_or(0);
            let label = parsed["name"].as_str().unwrap_or("(default)");
            println!(
                "Checkpoint {} saved: {} parts",
                label.cyan(),
                count.to_string().green()
            );
        }
    }
    Ok(())
}

pub fn cmd_restore(args: &RestoreArgs) -> Result<()> {
    let script = gdscript::generate_restore(args.name.as_deref());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let count = parsed["parts_restored"].as_u64().unwrap_or(0);
            let label = parsed["name"].as_str().unwrap_or("(default)");
            println!(
                "Restored {} parts from checkpoint {}",
                count.to_string().green(),
                label.cyan()
            );
        }
    }
    Ok(())
}
