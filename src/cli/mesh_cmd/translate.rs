use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{OutputFormat, TranslateArgs, parse_3d, run_eval};

pub fn cmd_translate(args: &TranslateArgs) -> Result<()> {
    let (x, y, z) = parse_3d(&args.to)?;
    let script = if let Some(ref ref_part) = args.relative_to {
        gdscript::generate_translate_relative_to(args.part.as_deref(), ref_part, x, y, z)
    } else {
        gdscript::generate_translate(args.part.as_deref(), x, y, z, args.relative)
    };
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let new_pos = parsed["position"].as_array();
            if let Some(pos) = new_pos {
                if let Some(ref_name) = parsed["relative_to"].as_str() {
                    println!(
                        "Translated {} relative to {}: ({:.2}, {:.2}, {:.2})",
                        name.green().bold(),
                        ref_name.cyan(),
                        pos[0].as_f64().unwrap_or(0.0),
                        pos[1].as_f64().unwrap_or(0.0),
                        pos[2].as_f64().unwrap_or(0.0),
                    );
                } else {
                    println!(
                        "Translated {}: ({:.2}, {:.2}, {:.2})",
                        name.green().bold(),
                        pos[0].as_f64().unwrap_or(0.0),
                        pos[1].as_f64().unwrap_or(0.0),
                        pos[2].as_f64().unwrap_or(0.0),
                    );
                }
            }
        }
    }
    Ok(())
}
