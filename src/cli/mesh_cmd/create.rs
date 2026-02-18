use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{CreateArgs, OutputFormat, run_eval};

pub fn cmd_create(args: &CreateArgs) -> Result<()> {
    let script = gdscript::generate_create(&args.name, args.from.as_str());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let prim = parsed["primitive"].as_str().unwrap_or("?");
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            println!(
                "Mesh session started: {} (primitive: {}, vertices: {vc})",
                name.green().bold(),
                prim.cyan()
            );
        }
    }
    Ok(())
}
