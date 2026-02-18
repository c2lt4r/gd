use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{AddPartArgs, OutputFormat, run_eval};

pub fn cmd_add_part(args: &AddPartArgs) -> Result<()> {
    let script = gdscript::generate_add_part(&args.name, args.from.as_str());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let active = parsed["active"].as_str().unwrap_or("?");
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            let parts: Vec<&str> = parsed["parts"]
                .as_array()
                .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            println!(
                "Added part: {} (active: {}, vertices: {vc}, parts: {})",
                name.green().bold(),
                active.cyan(),
                parts.join(", ").cyan()
            );
        }
    }
    Ok(())
}
