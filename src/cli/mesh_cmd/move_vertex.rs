use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{MoveVertexArgs, OutputFormat, parse_3d, run_eval};

pub fn cmd_move_vertex(args: &MoveVertexArgs) -> Result<()> {
    let (dx, dy, dz) = parse_3d(&args.delta)?;
    let script = gdscript::generate_move_vertex(args.index, dx, dy, dz);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let idx = parsed["index"].as_u64().unwrap_or(0);
            println!(
                "Moved vertex {}: delta=({dx}, {dy}, {dz})",
                idx.to_string().green().bold()
            );
        }
    }
    Ok(())
}
