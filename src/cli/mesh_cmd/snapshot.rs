use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, SnapshotArgs, run_eval};
use crate::cprintln;

pub fn cmd_snapshot(args: &SnapshotArgs) -> Result<()> {
    // Convert path to res:// if needed
    let tscn_path = if args.path.starts_with("res://") {
        args.path.clone()
    } else {
        format!("res://{}", args.path)
    };

    if !std::path::Path::new(&tscn_path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("tscn"))
    {
        return Err(miette!(
            "Snapshot path must end with .tscn, got: {tscn_path}"
        ));
    }

    if args.dry_run {
        let output = serde_json::json!({
            "path": tscn_path,
            "dry_run": true,
        });
        match args.format {
            OutputFormat::Json => {
                cprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
            }
            OutputFormat::Text => {
                cprintln!("{} Would save: {}", "Dry run:".yellow(), tscn_path.cyan());
            }
        }
        return Ok(());
    }

    let script = gdscript::generate_snapshot(&tscn_path);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let path = parsed["path"].as_str().unwrap_or("?");
            let count = parsed["part_count"].as_u64().unwrap_or(0);
            let parts: Vec<String> = parsed["parts"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v["name"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            cprintln!(
                "Snapshot saved: {} ({count} part{}: {})",
                path.green().bold(),
                if count == 1 { "" } else { "s" },
                parts.join(", ").cyan()
            );
        }
    }
    Ok(())
}
