use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{ListVerticesArgs, OutputFormat, parse_3d, run_eval};

pub fn cmd_list_vertices(args: &ListVerticesArgs) -> Result<()> {
    let region = if let Some(ref r) = args.region {
        let parts: Vec<&str> = r.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(miette::miette!(
                "Region must be two 3D points: \"x1,y1,z1 x2,y2,z2\""
            ));
        }
        let p1 = parse_3d(parts[0])?;
        let p2 = parse_3d(parts[1])?;
        Some((p1, p2))
    } else {
        None
    };

    let script = gdscript::generate_list_vertices(region.as_ref());
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let total = parsed["total_vertices"].as_u64().unwrap_or(0);
            let returned = parsed["returned"].as_u64().unwrap_or(0);
            println!(
                "{}: {returned}/{total} vertices{}",
                name.green().bold(),
                if args.region.is_some() {
                    " (filtered)"
                } else {
                    ""
                }
            );
            if let Some(verts) = parsed["vertices"].as_array() {
                for v in verts {
                    let idx = v["index"].as_u64().unwrap_or(0);
                    if let Some(pos) = v["position"].as_array() {
                        println!(
                            "  [{idx}]: ({:.4}, {:.4}, {:.4})",
                            pos[0].as_f64().unwrap_or(0.0),
                            pos[1].as_f64().unwrap_or(0.0),
                            pos[2].as_f64().unwrap_or(0.0),
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
