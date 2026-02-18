use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, SubdivideArgs, run_eval};

pub fn cmd_subdivide(args: &SubdivideArgs) -> Result<()> {
    let script = gdscript::generate_subdivide(args.part.as_deref(), args.iterations);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let iters = parsed["iterations"].as_u64().unwrap_or(0);
            let faces = parsed["face_count"].as_u64().unwrap_or(0);
            let verts = parsed["vertex_count"].as_u64().unwrap_or(0);
            println!(
                "Subdivided {} ({iters} iteration{}): {faces} faces, {verts} vertices",
                name.cyan(),
                if iters == 1 { "" } else { "s" }
            );
        }
    }
    Ok(())
}
