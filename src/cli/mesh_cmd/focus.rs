use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{FocusArgs, OutputFormat, run_eval};

pub fn cmd_focus(args: &FocusArgs) -> Result<()> {
    if !args.all && args.part.is_none() {
        return Err(miette::miette!(
            "Provide a part name or use --all to show all parts"
        ));
    }

    let script = if args.all {
        gdscript::generate_focus_all()
    } else {
        gdscript::generate_focus(args.part.as_deref().unwrap())
    };

    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let active = parsed["active"].as_str().unwrap_or("?");
            let parts: Vec<&str> = parsed["parts"]
                .as_array()
                .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            if args.all {
                println!(
                    "Showing {} parts (active: {})",
                    "all".green().bold(),
                    active.cyan()
                );
            } else {
                let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
                println!(
                    "Focused: {} (vertices: {vc}, parts: {})",
                    active.green().bold(),
                    parts.join(", ").cyan()
                );
            }
        }
    }
    Ok(())
}
