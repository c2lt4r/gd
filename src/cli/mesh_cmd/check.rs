use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{CheckArgs, OutputFormat, run_eval};

pub fn cmd_check(args: &CheckArgs) -> Result<()> {
    let script = gdscript::generate_check_floating(args.margin);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let ok = parsed["ok"].as_bool().unwrap_or(false);
            let total = parsed["total_parts"].as_u64().unwrap_or(0);
            let floating = parsed["floating"].as_array();
            if ok {
                println!(
                    "{} All {total} parts are connected (margin={:.1})",
                    "OK".green().bold(),
                    args.margin
                );
            } else {
                let count = floating.map_or(0, Vec::len);
                println!(
                    "{} {count}/{total} floating parts detected (margin={:.1}):",
                    "WARN".yellow().bold(),
                    args.margin
                );
                if let Some(names) = floating {
                    for n in names {
                        if let Some(name) = n.as_str() {
                            println!("  - {}", name.red());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
