use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{FocusArgs, OutputFormat, run_eval};
use crate::cprintln;

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
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let active = parsed["active"].as_str().unwrap_or("?");
            if args.all {
                let pc = parsed["part_count"].as_u64().unwrap_or(0);
                cprintln!(
                    "Showing {} parts (active: {})",
                    pc.to_string().green().bold(),
                    active.cyan()
                );
            } else {
                let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
                cprintln!("Focused: {} ({vc} vertices)", active.green().bold(),);
            }
        }
    }
    Ok(())
}
