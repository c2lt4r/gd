use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{FlipNormalsArgs, OutputFormat, run_eval};

pub fn cmd_flip_normals(args: &FlipNormalsArgs) -> Result<()> {
    if args.all {
        return cmd_flip_normals_all(args);
    }
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

fn cmd_flip_normals_all(args: &FlipNormalsArgs) -> Result<()> {
    let script = gdscript::generate_flip_normals_all();
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let count = parsed["parts_flipped"].as_u64().unwrap_or(0);
            println!("Flipped normals on {} parts:", count.to_string().green());
            if let Some(results) = parsed["results"].as_array() {
                for r in results {
                    let name = r["name"].as_str().unwrap_or("?");
                    let faces = r["face_count"].as_u64().unwrap_or(0);
                    println!("  {}: {faces} faces", name.cyan());
                }
            }
        }
    }
    Ok(())
}
