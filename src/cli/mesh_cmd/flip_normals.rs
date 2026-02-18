use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{FlipNormalsArgs, OutputFormat, run_eval};

pub fn cmd_flip_normals(args: &FlipNormalsArgs) -> Result<()> {
    let caps = args.caps.as_ref().map(super::Axis::as_str);
    let script = gdscript::generate_flip_normals(args.part.as_deref(), caps);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let flipped = parsed["flipped_faces"].as_u64().unwrap_or(0);
            let total = parsed["face_count"].as_u64().unwrap_or(0);
            if caps.is_some() {
                println!(
                    "Flipped {flipped}/{total} cap faces on {}",
                    name.green().bold()
                );
            } else {
                println!(
                    "Flipped normals on {}: {total} faces",
                    name.green().bold()
                );
            }
        }
    }
    Ok(())
}
