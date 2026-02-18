use miette::Result;
use owo_colors::OwoColorize;

use super::gdscript;
use super::{InfoArgs, OutputFormat, run_eval};

pub fn cmd_info(args: &InfoArgs) -> Result<()> {
    let script = if args.all {
        gdscript::generate_info_all()
    } else {
        gdscript::generate_info()
    };
    let result = run_eval(&script)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&result).map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            if args.all {
                print_info_all(&parsed);
            } else {
                print_info_single(&parsed);
            }
        }
    }
    Ok(())
}

fn print_info_single(parsed: &serde_json::Value) {
    let name = parsed["name"].as_str().unwrap_or("?");
    let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
    let fc = parsed["face_count"].as_u64().unwrap_or(0);
    let plane = parsed["profile_plane"].as_str().unwrap_or("none");
    let pts = parsed["profile_point_count"].as_u64().unwrap_or(0);
    println!(
        "Mesh: {} (vertices: {vc}, faces: {fc})",
        name.green().bold()
    );
    if let Some(pos) = parsed["aabb_position"].as_array() {
        let end = parsed["aabb_end"].as_array();
        if let Some(end) = end {
            println!(
                "AABB: ({:.2}, {:.2}, {:.2}) -> ({:.2}, {:.2}, {:.2})",
                pos[0].as_f64().unwrap_or(0.0),
                pos[1].as_f64().unwrap_or(0.0),
                pos[2].as_f64().unwrap_or(0.0),
                end[0].as_f64().unwrap_or(0.0),
                end[1].as_f64().unwrap_or(0.0),
                end[2].as_f64().unwrap_or(0.0),
            );
        }
    }
    if pts > 0 {
        println!("Profile: {plane} ({pts} points)");
    }
}

fn print_info_all(parsed: &serde_json::Value) {
    let active = parsed["active"].as_str().unwrap_or("?");
    let count = parsed["part_count"].as_u64().unwrap_or(0);
    let total_vc = parsed["total_vertex_count"].as_u64().unwrap_or(0);
    let total_fc = parsed["total_face_count"].as_u64().unwrap_or(0);
    println!(
        "{count} parts, {total_vc} vertices, {total_fc} faces (active: {})",
        active.cyan()
    );
    if let Some(parts) = parsed["parts"].as_array() {
        for p in parts {
            let name = p["name"].as_str().unwrap_or("?");
            let vc = p["vertex_count"].as_u64().unwrap_or(0);
            let fc = p["face_count"].as_u64().unwrap_or(0);
            let marker = if name == active { " *" } else { "" };
            println!("  {}{marker}: {vc} vertices, {fc} faces", name.green());
            if let Some(pos) = p["position"].as_array() {
                let fmt_vec = |arr: &[serde_json::Value]| -> String {
                    format!(
                        "{:.2}, {:.2}, {:.2}",
                        arr[0].as_f64().unwrap_or(0.0),
                        arr[1].as_f64().unwrap_or(0.0),
                        arr[2].as_f64().unwrap_or(0.0),
                    )
                };
                let rot = p["rotation"].as_array();
                let scl = p["scale"].as_array();
                println!(
                    "    pos({})  rot({})  scale({})",
                    fmt_vec(pos),
                    rot.map_or_else(|| "?".to_string(), |a| fmt_vec(a)),
                    scl.map_or_else(|| "?".to_string(), |a| fmt_vec(a)),
                );
            }
        }
    }
}
