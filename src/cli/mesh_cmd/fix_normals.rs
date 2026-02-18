use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{FixNormalsArgs, OutputFormat, run_eval};

pub fn cmd_fix_normals(args: &FixNormalsArgs) -> Result<()> {
    if args.all {
        return cmd_fix_normals_all(args);
    }
    let script = gdscript::generate_fix_normals(args.part.as_deref());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let flipped = parsed["faces_flipped"].as_u64().unwrap_or(0);
            let total = parsed["total_faces"].as_u64().unwrap_or(0);
            println!(
                "Fixed normals on {}: {}/{} faces corrected",
                name.cyan(),
                flipped.to_string().green(),
                total
            );
        }
    }
    Ok(())
}

fn cmd_fix_normals_all(args: &FixNormalsArgs) -> Result<()> {
    let script = gdscript::generate_fix_normals_all();
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let count = parsed["parts_fixed"].as_u64().unwrap_or(0);
            println!("Fixed normals on {} parts:", count.to_string().green());
            if let Some(results) = parsed["results"].as_array() {
                for r in results {
                    let name = r["name"].as_str().unwrap_or("?");
                    let flipped = r["faces_flipped"].as_u64().unwrap_or(0);
                    let total = r["total_faces"].as_u64().unwrap_or(0);
                    println!(
                        "  {}: {}/{} faces corrected",
                        name.cyan(),
                        flipped.to_string().green(),
                        total
                    );
                }
            }
        }
    }
    Ok(())
}
