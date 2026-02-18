use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{FlipNormalsArgs, OutputFormat, run_eval};

pub fn cmd_flip_normals(args: &FlipNormalsArgs) -> Result<()> {
    let script = gdscript::generate_flip_normals(args.part.as_deref());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let fc = parsed["face_count"].as_u64().unwrap_or(0);
            println!(
                "Flipped normals on {}: {fc} faces",
                name.green().bold()
            );
        }
    }
    Ok(())
}
